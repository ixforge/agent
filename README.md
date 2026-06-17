# IXForge Agent

Daemon ligero en Rust que gestiona la configuracion BIRD en route servers de un IXP. Parte del ecosistema [IXForge](https://github.com/ixforge).

Pollea configuracion desde el Core, la valida con `bird -p`, la instala atomicamente y la aplica enviando `configure` por el socket de control de BIRD, y reporta el estado de las sesiones BGP de vuelta al Core. Expone metricas Prometheus y endpoint de health.

## Componentes del ecosistema

- [Core](https://github.com/ixforge/core) — API REST, logica de negocio, base de datos
- **Agent** (este repo) — Daemon Rust que aplica configs BIRD en route servers
- [Collector](https://github.com/ixforge/collector) — Daemon Python que recolecta metricas SNMP/ICMP
- [E2E](https://github.com/ixforge/e2e) — Tests end-to-end del pipeline completo

## Requisitos

- BIRD 2.x instalado y corriendo
- Instancia de IXForge Core accesible
- Linux amd64
- Acceso root (para gestionar configuracion BIRD)
- Una API key con scope `agent:route_server` vinculada a este route server,
  creada en el Core con `POST /api/v1/route-servers/{id}/api-keys` (la key
  cruda se devuelve una sola vez)

## Instalacion

```bash
# Descargar binario
wget https://github.com/ixforge/agent/releases/latest/download/ixforge-agent
chmod +x ixforge-agent
sudo mv ixforge-agent /usr/local/bin/

# Configuracion
sudo mkdir -p /etc/ixforge-agent
# Copiar y editar config.toml (ver seccion Configuracion)

# Systemd
sudo cp ixforge-agent.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ixforge-agent
```

## Configuracion

Crear `/etc/ixforge-agent/config.toml`:

```toml
[core]
url = "https://core.tuixp.net"
api_key = "ixf_ag_xxxxxxxxxxxx"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 30
# ca_cert_path = "/etc/ixforge-agent/ca.pem"   # opcional, CA interna para TLS

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"
bird_binary = "/usr/sbin/bird"     # opcional, default /usr/sbin/bird (usado para bird -p)
socket_timeout_secs = 30           # opcional, default 30

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
# file_path = "/var/log/ixforge-agent.log"   # opcional, ademas de stdout
```

Ver `config.toml.example` para la referencia completa de campos.

## Endpoints

Servidos en la direccion de `[metrics] listen` (por defecto `:9100`):

- `GET /health` — JSON con `version`, `uptime_seconds`, `bird.running`, `core_connected`
- `GET /metrics` — Metricas Prometheus:
  - `ixforge_agent_uptime_seconds`
  - `ixforge_agent_poll_errors_total`
  - `ixforge_agent_bgp_sessions_up`, `ixforge_agent_bgp_sessions_total`
  - `ixforge_agent_host_cpu_usage_percent`, `ixforge_agent_host_memory_usage_percent`
  - `ixforge_agent_config_last_applied_timestamp`
  - `ixforge_agent_config_info{config_hash}`
  - `ixforge_agent_bgp_session_state{peer,asn}` (1=up, 0=down, por peer)
  - `ixforge_agent_bgp_prefixes_imported{peer,asn}`, `ixforge_agent_bgp_prefixes_exported{peer,asn}`

Si BIRD no esta corriendo el agent no se cae: el health reporta `bird.running:false`,
sigue enviando heartbeats y reintenta aplicar config en cada ciclo.

## Desarrollo

Requiere toolchain Rust >= 1.91 (edition 2024).

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
```

Sin toolchain local, compilar en Docker:

```bash
docker run --rm -v "$PWD":/src -w /src rust:1-bookworm cargo build --release
```

Tests de integracion contra BIRD real en Docker:

```bash
docker compose -f docker/docker-compose.test.yml up -d --build
BIRD_SOCKET=/tmp/ixforge-bird-test/bird.ctl cargo test --test integration_test -- --ignored
docker compose -f docker/docker-compose.test.yml down
```

## Licencia

Apache 2.0
