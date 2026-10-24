# IXForge Agent

Daemon ligero en Rust que gestiona la configuración de BIRD en los route servers de un IXP. Es parte del ecosistema [IXForge](https://github.com/ixforge).

Consulta periódicamente la configuración en el Core, la valida con `bird -p`, la instala de forma atómica y la aplica enviando `configure` por el socket de control de BIRD. Reporta al Core el resultado de cada aplicación, el estado de las sesiones BGP, los prefijos que anuncia cada peer y un heartbeat. Expone métricas Prometheus y un endpoint de health.

## Componentes del ecosistema

- [Core](https://github.com/ixforge/core): API REST, lógica de negocio, base de datos
- **Agent** (este repo): daemon Rust que aplica configs BIRD en los route servers
- [Collector](https://github.com/ixforge/collector): daemon Python que recolecta métricas SNMP/ICMP
- [E2E](https://github.com/ixforge/e2e): tests end-to-end del pipeline completo

## Requisitos

- Linux amd64 (el target que compila el CI)
- BIRD 2.x instalado: el agent usa el binario para `bird -p` y el socket de control para aplicar y consultar
- Una instancia de IXForge Core accesible
- Acceso root: la unit de systemd ejecuta el agent como root para escribir la configuración de BIRD y usar su socket
- Una API key con scope `agent:route_server` vinculada a este route server,
  creada en el Core con `POST /api/v1/route-servers/{id}/api-keys` (la key
  cruda se devuelve una sola vez)

## Instalación

No hay releases publicados: el binario se compila desde este repo (ver
[Desarrollo](#desarrollo)) o se descarga el artifact `ixforge-agent-linux-amd64`
de una ejecución del workflow de CI.

```bash
# En una máquina con Docker: compilar el binario
docker run --rm -v "$PWD":/src -w /src rust:1-bookworm cargo build --release

# En el route server: instalar el binario
sudo install -m 755 target/release/ixforge-agent /usr/local/bin/ixforge-agent

# Configuración (el archivo contiene la API key)
sudo mkdir -p /etc/ixforge-agent
sudo install -m 600 config.toml.example /etc/ixforge-agent/config.toml
# Editar config.toml (ver sección Configuración)

# Systemd
sudo cp ixforge-agent.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ixforge-agent
```

La unit ejecuta `/usr/local/bin/ixforge-agent --config /etc/ixforge-agent/config.toml`
después de `bird.service` y usa `ProtectSystem=strict` con
`ReadWritePaths=/etc/bird /run/bird`. Si `bird.config_path` o `bird.socket_path`
quedan fuera de esas rutas, o si se configura `logging.file_path`, hay que
agregar la ruta a `ReadWritePaths` (la unit trae comentada la variante con `/var/log`).

## Configuración

Crear `/etc/ixforge-agent/config.toml` (la ruta por defecto de `--config`):

```toml
[core]
url = "https://core.tuixp.net"
api_key = "ixf_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 30                        # opcional, default 30
# ca_cert_path = "/etc/ixforge-agent/ca.pem"   # opcional, CA interna para TLS

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"
bird_binary = "/usr/sbin/bird"   # opcional, default /usr/sbin/bird (se usa para bird -p)
socket_timeout_secs = 30         # opcional, default 30
max_response_mb = 128            # opcional, default 128: tope de una respuesta del socket

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"                                 # opcional, default info
format = "json"                                # opcional, default json; otro valor da texto plano
# file_path = "/var/log/ixforge-agent.log"     # opcional, además de stdout
```

Las cuatro secciones son obligatorias (`[logging]` puede quedar vacía para usar
los defaults). Al arrancar se valida que `poll_interval_secs` sea mayor que 0 y
que `core.url`, `core.api_key`, `bird.socket_path`, `bird.config_path` y
`bird.bird_binary` no estén vacíos; `route_server_id` tiene que ser un UUID.

- `ca_cert_path`: certificado PEM que se agrega a las CA de confianza del cliente HTTP.
- `config_path`: el agent escribe `<config_path>.tmp` en el mismo directorio y
  lo renombra sobre el archivo final, así que ese directorio tiene que ser escribible.
- `max_response_mb`: tiene que alcanzar para el dump de rutas del peer más
  grande (una tabla nacional pasa de 45 MB). Existe para que un BIRD que nunca
  envía el marcador de fin no haga crecer la memoria sin límite.
- `level`: acepta un nivel (`trace`, `debug`, `info`, `warn`, `error`) o una
  directiva de `tracing` como `ixforge_agent=debug`; si es inválido se usa `info`.

`config.toml.example` trae la misma referencia comentada, salvo `max_response_mb`.

## Ciclo de trabajo

Cada `poll_interval_secs` el agent ejecuta, en orden:

1. **Configuración.** Pide `GET /api/v1/route-servers/{id}/agent/config`. Si el
   hash difiere del último aplicado, escribe el contenido en `<config_path>.tmp`
   (con fsync), lo valida con `bird -p -c`, lo renombra sobre `config_path`
   (con fsync del directorio) y envía `configure` por el socket. Tanto
   `0003 Reconfigured` como `0004 Reconfiguration in progress` cuentan como éxito.
   Si algo falla, reporta el motivo a `.../agent/config/failed` (hasta 4000
   caracteres) para que se vea en el portal y vuelve a intentarlo en el
   siguiente ciclo. Si lo que falla es `configure`, el archivo nuevo ya quedó
   instalado en `config_path` (pasó `bird -p`).
2. **Confirmación.** Si hay una configuración aplicada sin confirmar, la confirma
   con `.../agent/config/applied`.
3. **Estado BGP.** Lee `show protocols all` y reporta las sesiones a `.../agent/status`.
4. **Prefijos.** Cada 10 ciclos, empezando por el primero, reporta a
   `.../agent/prefixes` los prefijos que anuncia cada sesión establecida, con su
   AS path y communities, a partir de `show route protocol <nombre> all`. Envía
   una sesión por request y omite las sesiones sin conteo de prefijos o con más
   de 500.000.
5. **Heartbeat.** Envía `.../agent/heartbeat` con la versión, el uptime, el hash
   de la configuración actual y el estado de BIRD. Si el Core responde con el
   header `X-IXForge-Agent-Upgrade`, el agent solo lo registra en el log.

Si BIRD no está en ejecución, el agent no se cae: el health reporta
`bird.running: false`, sigue enviando heartbeats y reintenta aplicar la
configuración en cada ciclo.

Si la configuración se aplica en BIRD pero falla la confirmación al Core, el agent
reintenta confirmarla en cada ciclo sin volver a aplicar la misma configuración,
incluso si falla la consulta de configuración. Una nueva configuración solo
reemplaza la confirmación pendiente cuando se aplica correctamente. La
confirmación pendiente vive en memoria: al reiniciar, el agent vuelve a
consultar y aplicar la configuración.

## Endpoints

Se sirven en la dirección de `[metrics] listen` (campo obligatorio; `:9100` en
el ejemplo y el puerto que verifica `deploy.sh`). Si no puede abrir esa
dirección, el agent lo registra en el log y sigue funcionando sin endpoints.

- `GET /health`: JSON con `version`, `uptime_seconds`, `bird.running` y
  `core_connected` (resultado de la última consulta de configuración al Core).
  Responde siempre HTTP 200.
- `GET /metrics`: métricas Prometheus:
  - `ixforge_agent_uptime_seconds`
  - `ixforge_agent_poll_errors_total`: fallos al consultar o aplicar la
    configuración
  - `ixforge_agent_bgp_sessions_up`, `ixforge_agent_bgp_sessions_total`
  - `ixforge_agent_host_cpu_usage_percent`, `ixforge_agent_host_memory_usage_percent`
  - `ixforge_agent_config_last_applied_timestamp`
  - `ixforge_agent_config_info{config_hash}` (desde la primera configuración aplicada)
  - `ixforge_agent_bgp_session_state{peer,asn}` (1 = up, 0 = cualquier otro estado, por peer)
  - `ixforge_agent_bgp_prefixes_imported{peer,asn}`, `ixforge_agent_bgp_prefixes_exported{peer,asn}`

## Desarrollo

Requiere toolchain Rust >= 1.91 (edition 2024, `rust-version` en `Cargo.toml`).

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Son los mismos chequeos que ejecuta el CI (`.github/workflows/ci.yml`), que
además compila el release para linux/amd64 y lo publica como artifact
`ixforge-agent-linux-amd64`.

Sin toolchain local, compilar en Docker:

```bash
docker run --rm -v "$PWD":/src -w /src rust:1-bookworm cargo build --release
```

Tests de integración contra un BIRD real en Docker (`cargo test` los deja como ignored):

```bash
docker compose -f docker/docker-compose.test.yml up -d --build
BIRD_SOCKET=/tmp/ixforge-bird-test/bird.ctl cargo test --test integration_test -- --ignored
docker compose -f docker/docker-compose.test.yml down
```

## Despliegue

`./deploy.sh <dev|prod> [--yes]` compila en Docker (`rust:1-bookworm`) el binario
del commit actual de HEAD y lo instala en los route servers del entorno: detiene
el servicio `ixforge-agent`, reemplaza `/usr/local/bin/ixforge-agent` y lo vuelve
a arrancar. BIRD no se toca. Aborta si hay cambios que no están en un commit.

Los route servers de cada entorno se definen en `deploy.env` (`DEV_ROUTE_SERVERS`
y `PROD_ROUTE_SERVERS`, separados por espacio; ver `deploy.env.example`). El
script entra como root por SSH con `ssh.exe` si está disponible, si no con `ssh`
(la variable `SSH` permite elegir otro). Al terminar verifica que cada route
server tenga un binario idéntico al compilado (sha256) y que `/health` responda
en el puerto 9100, y muestra `bird.running` y `core_connected` de cada uno. Para
prod pide escribir `prod` como confirmación (se omite con `--yes` o `-y`). El
flujo dev -> prod completo está en el repo `core` (`docs/staging.md`).

Para actualizar un IXP en producción entre versiones (los tres repos van en el
mismo tag), seguir la [guía de actualización](https://github.com/ixforge/core/blob/main/docs/migracion.md)
del repo `core`.

## Licencia

Apache 2.0
