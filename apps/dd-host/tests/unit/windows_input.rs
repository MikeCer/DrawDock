use super::*;

#[test]
fn blackboard_force_draw_releases_buttons_without_consuming_mode_request_sequence() {
    let mut state = PointerState::new();
    state.set_mode(Mode::Pointer, 9).unwrap();
    state.pointer("down", 1, 0.5, 0.5, "left").unwrap();
    let release = state.force_draw().unwrap().unwrap();
    assert_eq!(release.flags & MOUSEEVENTF_LEFTUP, MOUSEEVENTF_LEFTUP);
    assert_eq!(state.mode, Mode::Draw);
    assert!(state.active.is_none());
    assert!(state.set_mode(Mode::Pointer, 9).is_err());
    state.set_mode(Mode::Pointer, 10).unwrap();
}

#[test]
fn draw_mode_rejects_all_remote_pointer_commands() {
    let mut state = PointerState::new();
    assert!(state.pointer("down", 1, 0.5, 0.5, "left").is_err());
}

#[test]
fn stale_coordinates_and_overlapping_down_are_rejected() {
    let mut state = PointerState::new();
    state.set_mode(Mode::Pointer, 1).unwrap();
    assert!(state.pointer("hover", 0, f32::NAN, 0.5, "").is_err());
    assert!(state.pointer("down", 1, 0.5, 0.5, "left").is_ok());
    assert!(state.pointer("down", 2, 0.6, 0.6, "right").is_err());
    assert!(state.pointer("up", 0, 0.6, 0.6, "left").is_err());
}

#[test]
fn mode_change_releases_the_active_button_before_changing_mode() {
    let mut state = PointerState::new();
    state.set_mode(Mode::Pointer, 1).unwrap();
    state.pointer("down", 1, 0.5, 0.5, "right").unwrap();
    let release = state.set_mode(Mode::Draw, 2).unwrap();
    assert!(release.is_some());
    assert!(state.active.is_none());
    assert!(state.pointer("down", 2, 0.5, 0.5, "left").is_err());
}

#[test]
fn release_all_produces_a_matching_button_up_without_os_injection() {
    let mut state = PointerState::new();
    state.set_mode(Mode::Pointer, 1).unwrap();
    state.pointer("down", 1, 0.5, 0.5, "right").unwrap();
    let release = state.release_all().unwrap().unwrap();
    assert_eq!(release.flags & MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_RIGHTUP);
    assert!(state.active.is_none());
}
