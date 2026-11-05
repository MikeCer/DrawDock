use super::*;
#[test]
fn reverse_mapping_preserves_existing_and_rejects_conflicting_destination() {
    assert!(reverse_exists("device tcp:47831 tcp:47831", 47831).unwrap());
    assert!(!reverse_exists("device tcp:47832 tcp:47832", 47831).unwrap());
    assert!(reverse_exists("device tcp:47831 tcp:1000", 47831).is_err());
}
#[test]
fn device_discovery_keeps_authorization_state_and_excludes_network_adb() {
    let values = parse_devices(
        "List of devices attached\nabc device product:x\nbad unauthorized\n192.168.1.2:5555 device\n",
    );
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].serial, "abc");
    assert_eq!(values[1].state, "unauthorized");
}
