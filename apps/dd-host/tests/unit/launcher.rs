use super::*;
#[test]
fn tray_opens_on_single_double_and_keyboard_selection_with_versioned_notifications() {
    for event in [WM_LBUTTONUP, WM_LBUTTONDBLCLK, NIN_SELECT, NIN_KEYSELECT] {
        assert_eq!(tray_event(event as LPARAM), Some(TrayEvent::Open));
        assert_eq!(
            tray_event(((1u32 << 16) | event) as LPARAM),
            Some(TrayEvent::Open)
        );
    }
    for event in [WM_RBUTTONUP, WM_CONTEXTMENU] {
        assert_eq!(
            tray_event(((1u32 << 16) | event) as LPARAM),
            Some(TrayEvent::Menu)
        );
    }
    assert_eq!(tray_event(WM_MOUSEMOVE as LPARAM), None);
}

#[test]
fn logs_preserve_recent_order_and_bound_the_retained_history() {
    let mut logs = VecDeque::new();
    for value in 0..501 {
        append_logs(&mut logs, "time [HOST]", &value.to_string());
    }
    assert_eq!(logs.len(), 500);
    assert_eq!(logs.front().unwrap(), "time [HOST] 1");
    assert_eq!(logs.back().unwrap(), "time [HOST] 500");
}

#[test]
fn logs_redact_pairing_material_and_bound_long_unicode_lines() {
    let mut logs = VecDeque::new();
    append_logs(&mut logs, "time", "drawdock://pair?token=secret");
    assert!(!logs[0].contains("secret"));
    append_logs(&mut logs, "time", &"\u{e9}".repeat(5000));
    assert!(logs[1].ends_with("[truncated]"));
    assert!(logs[1].chars().count() < 4200);
}
