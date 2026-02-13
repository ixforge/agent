# IXForge Agent

IXForge Agent is a lightweight daemon that manages BIRD routing daemon configurations on IXP route servers. It continuously polls configuration updates from IXForge Core, validates them with `bird -p`, applies changes via `birdc configure`, and reports BGP session state back to Core. The agent exposes Prometheus metrics and health endpoints for monitoring.

## Prerequisites

- BIRD 2.x installed and running
- IXForge Core instance with API access
- Linux amd64 system
- Root access (required to manage BIRD configuration)

## Installation

1. Download the latest binary release:

```bash
wget https://github.com/ixforge/agent/releases/latest/download/ixforge-agent
chmod +x ixforge-agent
sudo mv ixforge-agent /usr/local/bin/
```

2. Create configuration directory:

```bash
sudo mkdir -p /etc/ixforge-agent
```

3. Create configuration file at `/etc/ixforge-agent/config.toml` (see Configuration section)

4. Install systemd service:

```bash
sudo cp ixforge-agent.service /etc/systemd/system/
sudo systemctl daemon-reload
```

## Configuration

Create `/etc/ixforge-agent/config.toml` with the following structure:

```toml
[core]
url = "https://core.ixforge.example.com"
api_key = "your-api-key-here"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 30
ca_cert_path = "/etc/ixforge-agent/ca.crt"  # Optional for self-signed certs

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"
bird_binary = "/usr/sbin/bird"  # Optional, defaults to /usr/sbin/bird

[metrics]
listen = "127.0.0.1:9091"

[logging]
level = "info"  # Options: trace, debug, info, warn, error
format = "json"  # Options: json, text
file_path = "/var/log/ixforge-agent.log"  # Optional, logs to stdout if omitted
```

### Configuration Options

**[core]**
- `url`: IXForge Core API URL
- `api_key`: Authentication key for Core API
- `route_server_id`: UUID identifying this route server in Core
- `poll_interval_secs`: Seconds between config polls (default: 30)
- `ca_cert_path`: Path to CA certificate for HTTPS validation (optional)

**[bird]**
- `socket_path`: Path to BIRD control socket
- `config_path`: Path to BIRD configuration file to manage
- `bird_binary`: Path to BIRD binary for validation (default: /usr/sbin/bird)

**[metrics]**
- `listen`: Address and port for metrics/health HTTP server

**[logging]**
- `level`: Log level (default: info)
- `format`: Log output format (default: json)
- `file_path`: Log file path (optional, logs to stdout if not specified)

## Running

### With systemd (recommended)

```bash
sudo systemctl enable ixforge-agent
sudo systemctl start ixforge-agent
sudo systemctl status ixforge-agent
```

View logs:

```bash
sudo journalctl -u ixforge-agent -f
```

### Manual execution

```bash
sudo /usr/local/bin/ixforge-agent --config /etc/ixforge-agent/config.toml
```

## Endpoints

The agent exposes two HTTP endpoints on the configured metrics listen address:

### Health Endpoint

`GET /health`

Returns JSON health status:

```json
{
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "bird": {
    "running": true
  },
  "core_connected": true
}
```

### Metrics Endpoint

`GET /metrics`

Returns Prometheus-formatted metrics:

```
# HELP ixforge_agent_bgp_sessions_up Number of BGP sessions in established state.
# TYPE ixforge_agent_bgp_sessions_up gauge
ixforge_agent_bgp_sessions_up 42

# HELP ixforge_agent_bgp_sessions_total Total number of BGP sessions.
# TYPE ixforge_agent_bgp_sessions_total gauge
ixforge_agent_bgp_sessions_total 50

# HELP ixforge_agent_poll_errors_total Total number of Core API poll errors.
# TYPE ixforge_agent_poll_errors_total counter
ixforge_agent_poll_errors_total 0

# HELP ixforge_agent_uptime_seconds Agent uptime in seconds.
# TYPE ixforge_agent_uptime_seconds gauge
ixforge_agent_uptime_seconds 3600

# HELP ixforge_agent_host_cpu_usage_percent Host CPU usage percentage.
# TYPE ixforge_agent_host_cpu_usage_percent gauge
ixforge_agent_host_cpu_usage_percent 12.5

# HELP ixforge_agent_host_memory_usage_percent Host memory usage percentage.
# TYPE ixforge_agent_host_memory_usage_percent gauge
ixforge_agent_host_memory_usage_percent 45.2
```

## How It Works

The agent runs a continuous polling loop with three main phases:

1. **Config Poll**: Fetches BIRD configuration from Core API and compares hash with current config
2. **Validate**: If config changed, writes to temporary file and validates with `bird -p`
3. **Apply**: If validation passes, writes to config path and applies via `birdc configure`
4. **Confirm**: Sends config applied confirmation to Core
5. **Status Report**: Parses BIRD protocol state via `birdc show protocols` and reports BGP session states to Core
6. **Heartbeat**: Sends periodic heartbeat with agent version, uptime, and BIRD instance status

The loop repeats every `poll_interval_secs` seconds.

## Troubleshooting

### BIRD socket not found

```
failed to get BIRD protocols: socket not found
```

**Solution**: Verify BIRD is running and socket path is correct:

```bash
sudo systemctl status bird
ls -l /run/bird/bird.ctl
```

Update `bird.socket_path` in config.toml if needed.

### Core API unreachable

```
failed to poll config from Core: connection refused
```

**Solution**: Check Core URL and network connectivity:

```bash
curl -v https://core.ixforge.example.com/health
```

Verify `core.url` and `core.api_key` in config.toml. Check firewall rules.

### Config validation failures

```
config validation failed, keeping previous config
```

**Solution**: The agent logs the full BIRD validation output. Check logs for syntax errors:

```bash
sudo journalctl -u ixforge-agent -n 50
```

BIRD validation errors indicate issues in the configuration generated by Core. Report these to your Core administrator.

### Certificate verification errors

```
failed to initialize Core API client: certificate verify failed
```

**Solution**: For self-signed certificates, set `core.ca_cert_path` to the CA certificate path:

```toml
[core]
ca_cert_path = "/etc/ixforge-agent/ca.crt"
```

## Development

### Build

```bash
cargo build --release
```

The binary will be in `target/release/ixforge-agent`.

### Test

```bash
cargo test
```

### Lint

```bash
cargo clippy -- -D warnings
```

### Format

```bash
cargo fmt
```

## License

Apache License 2.0

See LICENSE file for details.
