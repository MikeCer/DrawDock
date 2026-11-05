use super::*;

#[test]
fn invalid_or_missing_encoder_choice_is_rejected() {
    assert!(selected_video_encoder(&["serve".into(), "--video-encoder".into()]).is_err());
    assert!(
        selected_video_encoder(&["serve".into(), "--video-encoder".into(), "unknown".into()])
            .is_err()
    );
}

#[test]
fn pairing_info_output_is_opt_in_and_requires_a_path() {
    assert!(selected_pairing_info(&[]).unwrap().is_none());
    assert!(selected_pairing_info(&["serve".into(), "--pairing-info".into()]).is_err());
    assert_eq!(
        selected_pairing_info(&["serve".into(), "--pairing-info".into(), "pair.json".into()])
            .unwrap(),
        Some(PathBuf::from("pair.json"))
    );
}
