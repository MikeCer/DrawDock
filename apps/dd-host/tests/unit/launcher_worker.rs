use super::*;
#[test]
fn usb_reconnect_restores_only_the_selected_authorized_tablet_mapping() {
    let mut mapping = Reverse {
        adb: "adb".into(),
        serial: "selected".into(),
        port: 47831,
        created: false,
    };
    let mut calls = Vec::new();
    assert!(
        mapping
            .maintain(|_, args| {
                calls.push(
                    args.iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>(),
                );
                Ok(if args[0] == "devices" {
                    "List of devices attached\nselected device\n".into()
                } else {
                    String::new()
                })
            })
            .unwrap()
    );
    assert!(mapping.created);
    assert_eq!(
        calls.last().unwrap(),
        &[
            "-s",
            "selected",
            "reverse",
            "--no-rebind",
            "tcp:47831",
            "tcp:47831"
        ]
    );
    for devices in [
        "List of devices attached\nother device\n",
        "List of devices attached\nselected unauthorized\n",
        "List of devices attached\nselected offline\n",
    ] {
        let mut calls = 0;
        assert!(
            mapping
                .maintain(|_, _| {
                    calls += 1;
                    Ok(devices.into())
                })
                .is_err()
        );
        assert_eq!(calls, 1);
    }
    assert!(
        !mapping
            .maintain(|_, args| Ok(if args[0] == "devices" {
                "selected device".into()
            } else {
                "device tcp:47831 tcp:47831".into()
            }))
            .unwrap()
    );
    assert!(
        mapping
            .maintain(|_, args| Ok(if args[0] == "devices" {
                "selected device".into()
            } else {
                "device tcp:47831 tcp:1000".into()
            }))
            .is_err()
    );
}
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
