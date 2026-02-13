use ixforge_agent::config::AgentConfig;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn test_parse_minimal_config() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let config = AgentConfig::from_file(f.path()).unwrap();
    assert_eq!(config.core.url, "https://portal.example.net");
    assert_eq!(config.core.poll_interval_secs, 30);
    assert_eq!(config.core.ca_cert_path, None);
    assert_eq!(config.bird.bird_binary, "/usr/sbin/bird");
    assert_eq!(config.logging.file_path, None);
}

#[test]
fn test_parse_config_with_all_options() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 60
ca_cert_path = "/etc/ixforge-agent/ca.pem"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"
bird_binary = "/usr/local/sbin/bird"

[metrics]
listen = "127.0.0.1:9100"

[logging]
level = "debug"
format = "text"
file_path = "/var/log/ixforge-agent.log"
"#,
    );

    let config = AgentConfig::from_file(f.path()).unwrap();
    assert_eq!(config.core.poll_interval_secs, 60);
    assert_eq!(config.core.ca_cert_path.as_deref(), Some("/etc/ixforge-agent/ca.pem"));
    assert_eq!(config.bird.bird_binary, "/usr/local/sbin/bird");
    assert_eq!(config.logging.level, "debug");
    assert_eq!(config.logging.file_path.as_deref(), Some("/var/log/ixforge-agent.log"));
}

#[test]
fn test_parse_config_missing_required_field() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}

#[test]
fn test_parse_config_invalid_uuid() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "not-a-uuid"

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}

#[test]
fn test_parse_config_file_not_found() {
    let result = AgentConfig::from_file("/nonexistent/config.toml".as_ref());
    assert!(result.is_err());
}

#[test]
fn test_poll_interval_must_be_positive() {
    let f = write_config(
        r#"
[core]
url = "https://portal.example.net"
api_key = "ixf_ag_test123"
route_server_id = "550e8400-e29b-41d4-a716-446655440000"
poll_interval_secs = 0

[bird]
socket_path = "/run/bird/bird.ctl"
config_path = "/etc/bird/bird.conf"

[metrics]
listen = "0.0.0.0:9100"

[logging]
level = "info"
format = "json"
"#,
    );

    let result = AgentConfig::from_file(f.path());
    assert!(result.is_err());
}
