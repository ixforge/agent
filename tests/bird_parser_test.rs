use ixforge_agent::bird::parser::{parse_bird_uptime, parse_protocols, BgpState};

const BIRD_OUTPUT_MIXED: &str = r#"BIRD 2.15.1 ready.
Name       Proto      Table    State  Since       Info
device1    Device     ---      up     2024-01-15

direct1    Direct     ---      up     2024-01-15

peer_as64500 BGP        ---      up     2024-01-15  Established
  Description:    Peer AS64500
  BGP state:          Established
    Neighbor address: 10.0.0.1
    Neighbor AS:      64500
    Local AS:         65000

  Channel ipv4
    State:          UP
    Table:          master4
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         42 imported, 15 exported, 42 preferred

peer_as64501 BGP        ---      start  2024-01-15  Active
  Description:    Peer AS64501
  BGP state:          Active
    Neighbor address: 10.0.0.2
    Neighbor AS:      64501
    Local AS:         65000

  Channel ipv4
    State:          DOWN
    Table:          master4
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         0 imported, 0 exported, 0 preferred

peer_as64502_v6 BGP        ---      up     2024-01-14  Established
  Description:    Peer AS64502 IPv6
  BGP state:          Established
    Neighbor address: 2001:db8::1
    Neighbor AS:      64502
    Local AS:         65000

  Channel ipv6
    State:          UP
    Table:          master6
    Preference:     100
    Input filter:   ACCEPT
    Output filter:  ACCEPT
    Routes:         100 imported, 50 exported, 100 preferred
"#;

#[test]
fn test_parse_protocols_extracts_bgp_only() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(protocols.iter().all(|p| p.proto == "BGP"));
}

#[test]
fn test_parse_established_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64500").unwrap();
    assert_eq!(p.state, BgpState::Up);
    assert_eq!(p.neighbor_address.as_deref(), Some("10.0.0.1"));
    assert_eq!(p.neighbor_asn, Some(64500));
    assert_eq!(p.prefixes_imported, Some(42));
    assert_eq!(p.prefixes_exported, Some(15));
}

#[test]
fn test_parse_active_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64501").unwrap();
    assert_eq!(p.state, BgpState::Down);
    assert_eq!(p.neighbor_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(p.neighbor_asn, Some(64501));
    assert_eq!(p.prefixes_imported, Some(0));
    assert_eq!(p.prefixes_exported, Some(0));
}

#[test]
fn test_parse_ipv6_session() {
    let protocols = parse_protocols(BIRD_OUTPUT_MIXED).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64502_v6").unwrap();
    assert_eq!(p.state, BgpState::Up);
    assert_eq!(p.neighbor_address.as_deref(), Some("2001:db8::1"));
    assert_eq!(p.neighbor_asn, Some(64502));
    assert_eq!(p.prefixes_imported, Some(100));
    assert_eq!(p.prefixes_exported, Some(50));
}

#[test]
fn test_parse_empty_output() {
    let protocols = parse_protocols("BIRD 2.15.1 ready.\nName       Proto      Table    State  Since       Info\n").unwrap();
    assert!(protocols.is_empty());
}

#[test]
fn test_bgp_state_mapping() {
    assert_eq!(BgpState::from_bird_info("Established"), BgpState::Up);
    assert_eq!(BgpState::from_bird_info("Active"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("Connect"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("OpenSent"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("OpenConfirm"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("Idle"), BgpState::Down);
    assert_eq!(BgpState::from_bird_info("SomethingWeird"), BgpState::Unknown);
}

#[test]
fn test_bgp_state_to_oper_state_string() {
    assert_eq!(BgpState::Up.as_oper_state(), "up");
    assert_eq!(BgpState::Down.as_oper_state(), "down");
    assert_eq!(BgpState::Unknown.as_oper_state(), "unknown");
}

// Raw BIRD socket output (with protocol codes) captured from a real BIRD instance
const BIRD_SOCKET_RAW: &str = "\
2002-Name       Proto      Table      State  Since         Info
1002-device1    Device     ---        up     02:48:05.545
1006-
1002-direct1    Direct     ---        up     02:48:05.545
1006-  Channel ipv4
     State:          UP
     Table:          master4
     Preference:     240
     Input filter:   ACCEPT
     Output filter:  REJECT
     Routes:         1 imported, 0 exported, 1 preferred

1002-peer_as64500 BGP        ---        start  02:48:05.545  Idle
1006-  BGP state:          Idle
     Neighbor address: 10.99.0.1
     Neighbor AS:      64500
     Local AS:         65000
   Channel ipv4
     State:          DOWN
     Table:          master4
     Preference:     100
     Input filter:   ACCEPT
     Output filter:  ACCEPT

1002-peer_as64501 BGP        ---        start  02:48:05.545  Idle
1006-  BGP state:          Idle
     Neighbor address: 10.99.0.2
     Neighbor AS:      64501
     Local AS:         65000
   Channel ipv4
     State:          DOWN
     Table:          master4
     Preference:     100
     Input filter:   ACCEPT
     Output filter:  ACCEPT

1002-peer_as64502 BGP        ---        start  02:48:05.545  Idle
1006-  BGP state:          Idle
     Neighbor address: 10.99.0.3
     Neighbor AS:      64502
     Local AS:         65000
   Channel ipv4
     State:          DOWN
     Table:          master4
     Preference:     100
     Input filter:   ACCEPT
     Output filter:  ACCEPT

0000 \n";

#[test]
fn test_parse_raw_socket_output() {
    let protocols = parse_protocols(BIRD_SOCKET_RAW).unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(protocols.iter().all(|p| p.proto == "BGP"));
}

#[test]
fn test_parse_raw_socket_bgp_details() {
    let protocols = parse_protocols(BIRD_SOCKET_RAW).unwrap();
    let p = protocols.iter().find(|p| p.name == "peer_as64500").unwrap();
    assert_eq!(p.state, BgpState::Down);
    assert_eq!(p.neighbor_address.as_deref(), Some("10.99.0.1"));
    assert_eq!(p.neighbor_asn, Some(64500));
}

#[test]
fn test_parse_raw_socket_all_peers_found() {
    let protocols = parse_protocols(BIRD_SOCKET_RAW).unwrap();
    let names: Vec<&str> = protocols.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["peer_as64500", "peer_as64501", "peer_as64502"]);
}

const BIRD_OUTPUT_CONNECT_STATES: &str = r#"BIRD 2.15.1 ready.
Name       Proto      Table    State  Since       Info
peer_idle  BGP        ---      start  2024-01-15  Idle
  BGP state:          Idle
    Neighbor address: 10.0.0.10
    Neighbor AS:      64510
    Local AS:         65000

peer_opensent BGP        ---      start  2024-01-15  OpenSent
  BGP state:          OpenSent
    Neighbor address: 10.0.0.11
    Neighbor AS:      64511
    Local AS:         65000

peer_openconfirm BGP        ---      start  2024-01-15  OpenConfirm
  BGP state:          OpenConfirm
    Neighbor address: 10.0.0.12
    Neighbor AS:      64512
    Local AS:         65000
"#;

#[test]
fn test_parse_various_down_states() {
    let protocols = parse_protocols(BIRD_OUTPUT_CONNECT_STATES).unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(protocols.iter().all(|p| p.state == BgpState::Down));
}

const BIRD_STATUS_OUTPUT: &str = r#"BIRD 2.15.1 ready.
Router ID is 10.0.0.254
Hostname is rs1
Current server time is 2024-01-15 10:30:00.123
Last reboot on 2024-01-10 08:00:00.456
Last reconfiguration on 2024-01-15 09:00:00.789
Daemon is up and running
"#;

#[test]
fn test_parse_bird_uptime() {
    let uptime = parse_bird_uptime(BIRD_STATUS_OUTPUT).unwrap();
    // 5 days 2 hours 30 minutes = 5*86400 + 2*3600 + 30*60 = 440999.667
    let expected = 5.0 * 86400.0 + 2.0 * 3600.0 + 30.0 * 60.0 - 0.333;
    assert!((uptime - expected).abs() < 1.0, "expected ~{expected}, got {uptime}");
}

#[test]
fn test_parse_bird_uptime_no_fractional_seconds() {
    let output = "Current server time is 2024-01-15 10:30:00\nLast reboot on 2024-01-15 10:00:00\n";
    let uptime = parse_bird_uptime(output).unwrap();
    assert!((uptime - 1800.0).abs() < 0.01);
}

#[test]
fn test_parse_bird_uptime_missing_reboot() {
    let output = "Current server time is 2024-01-15 10:30:00\n";
    assert!(parse_bird_uptime(output).is_none());
}

// Raw socket output from `show status` (with protocol codes)
const BIRD_STATUS_RAW_SOCKET: &str = "\
1000-BIRD 2.15.1 ready.
1011-Router ID is 10.0.0.254
1011-Hostname is rs1
1011-Current server time is 2024-01-15 10:30:00.000
1011-Last reboot on 2024-01-15 10:00:00.000
1011-Last reconfiguration on 2024-01-15 10:15:00.000
0013 Daemon is up and running\n";

#[test]
fn test_parse_bird_uptime_raw_socket() {
    let uptime = parse_bird_uptime(BIRD_STATUS_RAW_SOCKET).unwrap();
    assert!((uptime - 1800.0).abs() < 0.01);
}
