use super::*;
use dd_scene::{InkObject, InkStyle, Point as ScenePoint, Tool, Transform};

#[test]
fn laser_interpolation_is_bounded_to_one_frame_without_extrapolation() {
    let started = Instant::now();
    let motion = LaserMotion {
        from: ScenePoint { x: 0.1, y: 0.2 },
        to: ScenePoint { x: 0.9, y: 0.6 },
        started,
    };
    assert_eq!(motion.at(started), motion.from);
    let halfway = motion.at(started + Duration::from_secs_f32(1.0 / 120.0));
    assert!((halfway.x - 0.5).abs() < 0.0001);
    assert!((halfway.y - 0.4).abs() < 0.0001);
    assert_eq!(motion.at(started + Duration::from_secs(1)), motion.to);
    let sprite = raster::laser_sprite([255, 0, 0, 128]).unwrap();
    assert_eq!(&sprite[(8 * 16 + 8) * 4..][..4], &[0, 0, 128, 128]);
    assert_eq!(&sprite[..4], &[0, 0, 0, 0]);
}

#[test]
fn rasterizes_scene_into_premultiplied_bgra_pixels() {
    let mut scene = Scene::default();
    scene
        .commit(InkObject {
            id: 1,
            tool: Tool::Freehand,
            style: InkStyle {
                rgba: [255, 0, 0, 128],
                width: 0.05,
            },
            points: vec![ScenePoint { x: 0.5, y: 0.5 }],
            text: None,
            image_data: None,
            transform: Transform::default(),
        })
        .unwrap();
    let mut pixels = vec![0; 20 * 20 * 4];
    render(&scene, 20, 20, &mut pixels).unwrap();
    let painted = &pixels[(9 * 20 + 9) * 4..(9 * 20 + 9) * 4 + 4];
    assert!(painted[2] > 0 && painted[2] == painted[3] && painted[0] == 0 && painted[1] == 0);
    let first_frame = pixels.clone();
    render(&scene, 20, 20, &mut pixels).unwrap();
    assert_eq!(pixels, first_frame);

    scene.clear().unwrap();
    render(&scene, 20, 20, &mut pixels).unwrap();
    assert!(pixels.iter().all(|pixel| *pixel == 0));
    scene.undo();
    render(&scene, 20, 20, &mut pixels).unwrap();
    assert_eq!(pixels, first_frame);
}
