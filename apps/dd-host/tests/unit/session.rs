use super::*;

#[test]
fn recovery_chunks_are_bounded_ordered_and_cannot_be_finished_as_commits() {
    let scene = Arc::new(Mutex::new(Scene::demo()));
    let mut input = test_input();
    let mut transaction = None;
    let bytes =
        serde_json::to_vec(&json!({"objects": [], "canvas": {"blackboard":false,"grid":false}}))
            .unwrap();
    let begin = json!({"type":"begin_restore","request_id":8,"total_bytes":bytes.len(),
        "digest":hex(&Sha256::digest(&bytes))});
    process_command(&begin, &scene, &mut input, &mut transaction).unwrap();
    assert!(transaction.as_ref().unwrap().restoring);
    assert!(
        process_command(
            &json!({"type":"commit_chunk","object_id":8,"offset":0,
        "data":STANDARD.encode(&bytes)}),
            &scene,
            &mut input,
            &mut transaction
        )
        .is_err()
    );
    assert!(transaction.is_none());
    process_command(&begin, &scene, &mut input, &mut transaction).unwrap();
    process_command(
        &json!({"type":"restore_chunk","request_id":8,"offset":0,
        "data":STANDARD.encode(&bytes)}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap();
    assert!(
        process_command(
            &json!({"type":"finish_commit","object_id":8}),
            &scene,
            &mut input,
            &mut transaction
        )
        .is_err()
    );
    assert_eq!(scene.lock().unwrap().objects().len(), 4);
    assert!(require_ready("begin_restore", false).is_err());
    assert!(
        process_command(
            &json!({"type":"begin_restore","request_id":8,"total_bytes":16*1024*1024+1,
        "digest":hex(&Sha256::digest(&bytes))}),
            &scene,
            &mut input,
            &mut transaction
        )
        .is_err()
    );
}

#[test]
fn commit_acknowledges_the_object_and_content_revision_but_rejection_never_does() {
    let scene = Arc::new(Mutex::new(Scene::default()));
    let mut input = test_input();
    let mut transaction = None;
    let object = serde_json::to_value(&Scene::demo().objects()[0]).unwrap();
    let message = json!({"type": "commit", "source_epoch": 7, "object": object});
    let ack = process_command(&message, &scene, &mut input, &mut transaction)
        .unwrap()
        .unwrap();
    assert_eq!(
        ack,
        json!({"type": "commit_ack", "object_id": 1, "source_epoch": 7, "revision": 1})
    );
    let error = process_command(&message, &scene, &mut input, &mut transaction).unwrap_err();
    let rejected = command_error(&message, &error);
    assert_eq!(rejected["object_id"], 1);
    assert_eq!(rejected["type"], "error");
    assert_eq!(scene.lock().unwrap().revision(), 1);
}
use std::net::{Ipv4Addr, Ipv6Addr};

fn test_input() -> WindowsInput {
    WindowsInput::new(OutputBounds {
        left: 0,
        top: 0,
        width: 200,
        height: 100,
    })
    .unwrap()
}

#[test]
fn editor_ack_contains_selection_bounds_canvas_mode_and_correlation() {
    let scene = Arc::new(Mutex::new(Scene::demo()));
    let mut input = test_input();
    let mut transaction = None;
    let selected = process_command(
        &json!({"type":"select_object","x":0.1,"y":0.72,"request_id":12,"source_epoch":3}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected["type"], "editor_ack");
    assert_eq!(selected["request_id"], 12);
    assert_eq!(selected["source_epoch"], 3);
    assert_eq!(selected["selection"]["id"], 1);
    assert_eq!(selected["mode"], "draw");
    assert_eq!(selected["canvas"], json!({"blackboard":false,"grid":false}));
    let transform = Transform {
        tx: 0.1,
        ..Default::default()
    };
    let transformed=process_command(&json!({"type":"transform_object","object_id":1,"transform":transform,"request_id":13,"source_epoch":3}),
            &scene,&mut input,&mut transaction).unwrap().unwrap();
    assert!((transformed["selection"]["bounds"]["left"].as_f64().unwrap() - 0.2).abs() < 0.00001);
    assert_eq!(transformed["selection"]["transform"], json!(transform));
    let missed = process_command(
        &json!({"type":"select_object","x":0.01,"y":0.01,"request_id":14,"source_epoch":3}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert!(missed["selection"].is_null());
    assert!(process_command(&json!({"type":"transform_object","object_id":1,"transform":{},"request_id":15,"source_epoch":3}),
            &scene,&mut input,&mut transaction).is_err());
}

#[test]
fn blackboard_forces_draw_and_clear_history_acknowledges_full_canvas_state() {
    let scene = Arc::new(Mutex::new(Scene::default()));
    let mut input = test_input();
    let mut transaction = None;
    input.set_mode(Mode::Pointer, 1).unwrap();
    let board = process_command(
        &json!({"type":"set_canvas","blackboard":true,"grid":true,"request_id":2,"source_epoch":4}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert_eq!(board["mode"], "draw");
    assert!(board["selection"].is_null());
    assert!(
        process_command(
            &json!({"type":"set_mode","mode":"pointer","request_id":3}),
            &scene,
            &mut input,
            &mut transaction
        )
        .is_err()
    );
    let clear = process_command(
        &json!({"type":"clear_display","request_id":4,"source_epoch":4}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert_eq!(clear["type"], "clear_ack");
    assert_eq!(clear["canvas"], json!({"blackboard":false,"grid":false}));
    assert_eq!(clear["source_epoch"], 4);
    input.set_mode(Mode::Pointer, 5).unwrap();
    let undo = process_command(
        &json!({"type":"undo","request_id":6,"source_epoch":4}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert_eq!(undo["type"], "ack");
    assert_eq!(undo["request_id"], 6);
    assert_eq!(undo["mode"], "draw");
    assert_eq!(undo["canvas"], json!({"blackboard":true,"grid":true}));
    let redo = process_command(
        &json!({"type":"redo","request_id":7,"source_epoch":4}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();
    assert_eq!(redo["canvas"], json!({"blackboard":false,"grid":false}));
    // Policy-enforced Draw did not consume a tablet mode request number.
    input.set_mode(Mode::Pointer, 8).unwrap();
}

#[test]
fn laser_is_draw_only_unacknowledged_transient_and_uses_default_red() {
    let scene = Arc::new(Mutex::new(Scene::default()));
    let mut input = test_input();
    let mut transaction = None;
    let active = json!({"type":"laser","active":true,"x":0.5,"y":0.5,"source_epoch":1});
    assert!(
        process_command(&active, &scene, &mut input, &mut transaction)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        scene.lock().unwrap().laser().unwrap().rgba,
        [255, 0, 0, 255]
    );
    assert_eq!(
        scene.lock().unwrap().revision(),
        0,
        "Laser motion must not invalidate scene raster"
    );
    assert!(!scene.lock().unwrap().undo());
    assert!(
        process_command(
            &json!({"type":"laser","active":false}),
            &scene,
            &mut input,
            &mut transaction
        )
        .unwrap()
        .is_none()
    );
    assert!(scene.lock().unwrap().laser().is_none());
    assert_eq!(scene.lock().unwrap().revision(), 0);
    input.set_mode(Mode::Pointer, 1).unwrap();
    assert!(process_command(&active, &scene, &mut input, &mut transaction).is_err());
}

#[test]
fn editing_requires_ready_geometry_and_errors_preserve_request_epoch() {
    for command in [
        "select_object",
        "transform_object",
        "set_canvas",
        "laser",
        "commit",
        "finish_commit",
        "undo",
        "redo",
        "clear",
        "clear_display",
        "set_mode",
        "set_zoom",
    ] {
        assert!(require_ready(command, false).is_err());
        require_ready(command, true).unwrap();
    }
    require_ready("heartbeat", false).unwrap();
    let error = command_error(
        &json!({"type":"transform_object","request_id":99,"source_epoch":7}),
        "bad matrix",
    );
    assert_eq!(
        error,
        json!({"type":"error","message":"bad matrix","request_id":99,"source_epoch":7})
    );
    let ready = source_ready_ack(
        &json!({"source_epoch":7}),
        Canvas {
            blackboard: true,
            grid: true,
        },
        Mode::Draw,
    );
    assert_eq!(
        ready,
        json!({"type":"source_ready_ack","source_epoch":7,"canvas":{"blackboard":true,"grid":true},"mode":"draw"})
    );
}

#[test]
fn only_loopback_and_private_addresses_are_allowed() {
    assert!(is_allowed_bind(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    assert!(is_allowed_bind(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 4))));
    assert!(!is_allowed_bind(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    assert!(!is_allowed_bind(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    assert!(is_allowed_bind(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    assert!(!is_allowed_bind(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
}

#[test]
fn pairing_tokens_are_compared_without_early_exit() {
    assert!(constant_time_eq(b"same", b"same"));
    assert!(!constant_time_eq(b"same", b"diff"));
    assert!(!constant_time_eq(b"short", b"longer"));
}

#[test]
fn pairing_requires_correct_unexpired_invitation_before_local_approval() {
    let token = "abcdef0123456789abcdef0123456789";
    let expires = Instant::now() + TOKEN_LIFETIME;
    assert!(valid_invitation(
        &json!({"type": "pair", "token": token}),
        token,
        expires,
    ));
    for request in [
        json!({"type": "pair", "token": "123456"}),
        json!({"type": "pair"}),
        json!({"type": "commit", "token": token}),
    ] {
        assert!(!valid_invitation(&request, token, expires));
    }
    assert!(!valid_invitation(
        &json!({"type": "pair", "token": token}),
        token,
        Instant::now() - Duration::from_secs(1),
    ));
}

#[test]
fn oversized_json_line_is_rejected_before_decode() {
    let mut reader = io::Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 1]);
    let error = read_json_line(&mut reader).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn clear_display_command_clears_the_shared_scene_and_acknowledges() {
    let scene = Arc::new(Mutex::new(Scene::demo()));
    let mut input = WindowsInput::new(OutputBounds {
        left: 0,
        top: 0,
        width: 100,
        height: 100,
    })
    .unwrap();
    let mut transaction = None;
    let response = process_command(
        &json!({"type": "clear_display", "request_id": 7}),
        &scene,
        &mut input,
        &mut transaction,
    )
    .unwrap()
    .unwrap();

    assert!(scene.lock().unwrap().objects().is_empty());
    assert_eq!(response["type"], "clear_ack");
    assert_eq!(response["request_id"], 7);
}
