// Integration tests require BIRD running in Docker
// Run: docker compose -f docker/docker-compose.test.yml up -d
// Then: BIRD_SOCKET=/tmp/ixforge-bird-test/bird.ctl cargo test --test integration_test -- --ignored

use ixforge_agent::bird::BirdClient;
use ixforge_agent::bird::parser::parse_protocols;
use ixforge_agent::bird::socket::BirdSocketClient;

fn bird_socket_path() -> String {
    std::env::var("BIRD_SOCKET").unwrap_or_else(|_| "/tmp/ixforge-bird-test/bird.ctl".to_string())
}

#[tokio::test]
#[ignore = "requires BIRD running in Docker"]
async fn test_bird_socket_connection() {
    let client = BirdSocketClient::new(&bird_socket_path(), 30);
    assert!(client.is_running().await, "BIRD should be running");
}

#[tokio::test]
#[ignore = "requires BIRD running in Docker"]
async fn test_bird_show_protocols() {
    let client = BirdSocketClient::new(&bird_socket_path(), 30);
    let output = client.send_command("show protocols all").await.unwrap();
    assert!(output.contains("device1"), "should list device protocol");
}

#[tokio::test]
#[ignore = "requires BIRD running in Docker"]
async fn test_bird_parse_real_socket_output() {
    let client = BirdSocketClient::new(&bird_socket_path(), 30);
    let output = client.send_command("show protocols all").await.unwrap();
    let protocols = parse_protocols(&output).unwrap();

    // Our test bird.conf has 3 BGP peers
    assert_eq!(protocols.len(), 3, "should find 3 BGP protocols");
    assert!(protocols.iter().all(|p| p.proto == "BGP"));

    let names: Vec<&str> = protocols.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"peer_as64500"));
    assert!(names.contains(&"peer_as64501"));
    assert!(names.contains(&"peer_as64502"));

    // All peers should have neighbor addresses parsed
    for p in &protocols {
        assert!(
            p.neighbor_address.is_some(),
            "{} should have neighbor address",
            p.name
        );
        assert!(
            p.neighbor_asn.is_some(),
            "{} should have neighbor ASN",
            p.name
        );
    }
}
