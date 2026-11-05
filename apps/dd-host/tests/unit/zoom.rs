use super::*;

#[test]
fn rejects_unbounded_nonfinite_and_outside_zoom() {
    for view in [
        Viewport {
            scale: 0.5,
            ..Viewport::default()
        },
        Viewport {
            scale: 4.1,
            ..Viewport::default()
        },
        Viewport {
            scale: f32::NAN,
            ..Viewport::default()
        },
        Viewport {
            scale: 2.0,
            left: 0.51,
            top: 0.0,
        },
        Viewport {
            scale: 2.0,
            left: -0.1,
            top: 0.0,
        },
        Viewport {
            scale: 2.0,
            left: 0.0,
            top: f32::INFINITY,
        },
    ] {
        assert!(view.validate().is_err());
    }
    assert!(Viewport::default().validate().is_ok());
    assert!(
        Viewport {
            scale: 4.0,
            left: 0.75,
            top: 0.75
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn pixel_quantization_stays_inside_monitor_and_matches_source_origin() {
    let view = Viewport {
        scale: 3.0,
        left: 2.0 / 3.0,
        top: 2.0 / 3.0,
    }
    .for_pixels(2560, 1440);
    assert!(view.validate().is_ok());
    assert_eq!((view.left * 2560.0).round(), 1706.0);
    assert_eq!((view.top * 1440.0).round(), 960.0);
}
