# IXForge Agent

Daemon ligero en Rust que gestiona la configuracion BIRD en route servers de un IXP. Parte del ecosistema [IXForge](https://github.com/ixforge).

Pollea configuracion desde el Core, la valida con `bird -p`, la aplica via `birdc configure`, y reporta el estado de las sesiones BGP de vuelta al Core. Expone metricas Prometheus y endpoint de health.

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
api_key = "tu-api-key"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 30

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "127.0.0.1:9091"

[logging]
level = "info"
format = "json"
```

## Endpoints

- `GET /health` — Estado del agent y de BIRD
- `GET /metrics` — Metricas Prometheus (sesiones BGP, prefijos, uptime, CPU/RAM)

## Desarrollo

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
```

Tests de integracion contra BIRD real en Docker:

```bash
docker compose -f docker/docker-compose.test.yml up -d --build
BIRD_SOCKET=/tmp/ixforge-bird-test/bird.ctl cargo test --test integration_test -- --ignored
docker compose -f docker/docker-compose.test.yml down
```

## Licencia

Apache 2.0
