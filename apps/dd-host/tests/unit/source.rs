use super::*;

#[test]
fn clock_discontinuities_fail_capture_but_a_new_video_session_can_recover() {
    let mut history = PresentationHistory::new();
    history.wall_origin -= Duration::from_secs(2);
    assert!(history.now().unwrap_err().contains("clock changed"));
    assert!(PresentationHistory::new().now().is_ok());
}

#[test]
fn captured_revision_never_crosses_an_uncompleted_or_uncertain_presentation() {
    let mut history = PresentationHistory::new();
    let state = |revision| Presentation {
        epoch: 1,
        revision,
        zoom: Viewport::default(),
    };
    history.record_at(100_000, state(1));
    assert!(history.at(100_000).is_none());
    assert_eq!(history.at(140_000), Some(&state(1)));
    history.transition_at(200_000);
    assert!(history.at(180_000).is_none());
    history.record_at(240_000, state(2));
    assert!(history.at(250_000).is_none());
    assert_eq!(history.at(280_000), Some(&state(2)));
    assert_eq!(history.at(150_000), Some(&state(1)));
    for revision in 3..200 {
        history.record_at(300_000 + revision * 100_000, state(revision));
    }
    assert_eq!(history.intervals.len(), MAX_PRESENTATIONS);
    assert!(history.at(150_000).is_none());
}

#[test]
fn source_switch_rejects_queued_old_geometry_and_requires_current_epoch() {
    let output = Output {
        name: "test".into(),
        left: -100,
        top: 0,
        right: 100,
        bottom: 100,
        primary: false,
    };
    let mut source = SelectedSource::new(output);
    let queued = json!({"source_epoch": 1});
    assert!(source.validate_epoch(&queued).is_ok());
    source.epoch += 1;
    assert!(source.validate_epoch(&queued).is_err());
    assert!(source.validate_epoch(&json!({})).is_err());
    assert!(source.validate_epoch(&json!({"source_epoch": 3})).is_err());
    assert!(source.validate_epoch(&json!({"source_epoch": 2})).is_ok());
}
