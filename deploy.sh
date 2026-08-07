#!/usr/bin/env bash
# Deploy del IXForge Agent a los route servers de un entorno
#
#   ./deploy.sh dev
#   ./deploy.sh prod
#   ./deploy.sh prod --yes    # sin confirmacion interactiva
#
# Compila el binario del commit actual de HEAD (aborta si hay cambios sin
# commitear) y lo instala en los dos route servers del entorno. BIRD no se toca
set -euo pipefail

# Route servers por entorno (separados por espacio). Se definen en deploy.env
# (gitignored, ver deploy.env.example)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ -f "$SCRIPT_DIR/deploy.env" ] && . "$SCRIPT_DIR/deploy.env"
declare -A ROUTE_SERVERS=(
  [dev]="${DEV_ROUTE_SERVERS:-}"
  [prod]="${PROD_ROUTE_SERVERS:-}"
)

BIN=target/release/ixforge-agent
REMOTE_BIN=/usr/local/bin/ixforge-agent
HEALTH_PORT=9100
BUILD_IMAGE=rust:1-bookworm

SSH_BIN="${SSH:-$(command -v ssh.exe 2>/dev/null || command -v ssh)}"
[[ -n "$SSH_BIN" ]] || { echo "no encuentro ssh" >&2; exit 1; }

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  B=$'\e[1m'; D=$'\e[2m'; R=$'\e[31m'; G=$'\e[32m'; Y=$'\e[33m'; C=$'\e[36m'; Z=$'\e[0m'
else
  B=; D=; R=; G=; Y=; C=; Z=
fi

TOTAL=4
step() { printf '\n%s[%d/%d]%s %s%s%s\n' "$C" "$1" "$TOTAL" "$Z" "$B" "$2" "$Z"; }
ok()   { printf '      %s✓%s %s\n' "$G" "$Z" "$1"; }
info() { printf '      %s·%s %s\n' "$D" "$Z" "$1"; }
warn() { printf '      %s!%s %s\n' "$Y" "$Z" "$1"; }
die()  { printf '\n%s✗ %s%s\n' "$R" "$1" "$Z" >&2; exit 1; }

TARGET="${1:-}"
ASSUME_YES=0
[[ "${2:-}" == "--yes" || "${2:-}" == "-y" ]] && ASSUME_YES=1

[[ -n "$TARGET" && -n "${ROUTE_SERVERS[$TARGET]:-}" ]] || die "uso: ./deploy.sh <${!ROUTE_SERVERS[*]}> [--yes]"
RS_LIST="${ROUTE_SERVERS[$TARGET]}"
ssh_run() { local ip=$1; shift; "$SSH_BIN" -o BatchMode=yes -o ConnectTimeout=15 "root@$ip" "$@"; }

cd "$(git rev-parse --show-toplevel)"
SHA=$(git rev-parse --short HEAD)
SUBJECT=$(git log -1 --format=%s)
TCOLOR=$C; [[ "$TARGET" == prod ]] && TCOLOR=$Y

printf '%sIXForge Agent%s %s·%s deploy → %s%s%s %s(%s)%s\n' "$B" "$Z" "$D" "$Z" "$TCOLOR" "$TARGET" "$Z" "$D" "$RS_LIST" "$Z"
printf '%scommit %s — %s%s\n' "$D" "$SHA" "$SUBJECT" "$Z"

# 1. Chequeos
step 1 "Chequeos previos"
git diff --quiet && git diff --cached --quiet || die "hay cambios sin commitear — commitea antes de deployar"
ok "working tree limpio (compilando $SHA)"
for ip in $RS_LIST; do
  ssh_run "$ip" true 2>/dev/null || die "no llego a root@$ip (VPN conectada?)"
done
ok "acceso a los route servers"
if [[ "$TARGET" == prod && "$ASSUME_YES" -eq 0 ]]; then
  printf '      %s! vas a deployar a PRODUCCION. escribi "prod" para confirmar: %s' "$Y" "$Z"
  read -r reply < /dev/tty
  [[ "$reply" == "prod" ]] || die "cancelado"
fi

# 2. Compilar
step 2 "Compilando el binario (release)"
docker run --rm -v "$PWD":/src -v ixforge-cargo-cache:/usr/local/cargo/registry \
  -w /src "$BUILD_IMAGE" cargo build --release >/dev/null 2>&1 || die "fallo la compilacion"
[[ -f "$BIN" ]] || die "no se genero $BIN"
LOCAL_SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
ok "binario compilado ($(du -h "$BIN" | cut -f1), sha ${LOCAL_SHA:0:12})"

# 3. Instalar en cada route server
step 3 "Instalando en los route servers"
for ip in $RS_LIST; do
  "$SSH_BIN" -o BatchMode=yes -o ConnectTimeout=15 "root@$ip" "
    set -e
    systemctl stop ixforge-agent
    cat > $REMOTE_BIN.new && chmod 755 $REMOTE_BIN.new && mv $REMOTE_BIN.new $REMOTE_BIN
    systemctl start ixforge-agent
  " < "$BIN"
  info "$ip actualizado"
done
ok "binario instalado en los ${RS_LIST// /, }"

# 4. Verificacion
step 4 "Verificando"
for ip in $RS_LIST; do
  RSHA=$(ssh_run "$ip" "sha256sum $REMOTE_BIN | cut -d' ' -f1")
  [[ "$RSHA" == "$LOCAL_SHA" ]] || die "$ip tiene un binario distinto al compilado"
  HEALTH=$(ssh_run "$ip" "for i in \$(seq 1 10); do curl -sf -m3 http://localhost:$HEALTH_PORT/health && exit 0; sleep 2; done; exit 1") \
    || die "$ip: el agent no respondio health"
  BIRD=$(printf '%s' "$HEALTH" | grep -o '"running":[a-z]*' | head -1 | cut -d: -f2)
  CONN=$(printf '%s' "$HEALTH" | grep -o '"core_connected":[a-z]*' | cut -d: -f2)
  ok "$ip: binario OK, bird running=$BIRD, core_connected=$CONN"
done

printf '\n%s✓ agent %s desplegado en los route servers de %s%s\n' "$G" "$SHA" "$TARGET" "$Z"
