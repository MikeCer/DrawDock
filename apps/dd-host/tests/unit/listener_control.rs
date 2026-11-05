use super::*;

#[test]
fn independent_listener_removal_cancels_only_its_active_connection() {
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = first.local_addr().unwrap();
    drop(first);
    let hub = Listeners::single(address).unwrap();
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    let other = second.local_addr().unwrap();
    drop(second);
    hub.enable(other).unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    let (tcp, _, endpoint) = hub.accept().unwrap().unwrap();
    let guard = hub.connection(endpoint, &tcp).unwrap();
    hub.disable(other).unwrap();
    assert!(hub.enabled(address).unwrap());
    assert!(!hub.enabled(other).unwrap());
    hub.disable(address).unwrap();
    use std::io::Read;
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    assert_eq!(client.read(&mut [0]).unwrap(), 0);
    drop(guard);
    assert!(hub.video_listener(address).is_err());
}

#[test]
fn refuses_wildcard_public_zero_port_and_unrecognized_commands() {
    let hub = Listeners::new(false);
    for address in ["0.0.0.0:1234", "8.8.8.8:1234", "127.0.0.1:0"] {
        assert!(hub.enable(address.parse().unwrap()).is_err());
    }
    assert!(
        serde_json::from_value::<Command>(
            json!({"action":"enable","endpoint":"127.0.0.1:1234","request_id":1,"unexpected":true})
        )
        .is_err()
    );
}
