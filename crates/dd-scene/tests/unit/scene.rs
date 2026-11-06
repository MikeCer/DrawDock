use super::*;

#[test]
fn recovery_restores_objects_and_canvas_atomically_without_old_history_or_laser() {
    let mut scene = Scene::demo();
    scene.clear().unwrap();
    scene
        .set_laser(true, Point { x: 0.5, y: 0.5 }, [255, 0, 0, 255])
        .unwrap();
    let revision = scene.revision();
    let objects = vec![stroke(17, 0.2), shape(Tool::Rectangle)];
    let canvas = Canvas {
        blackboard: true,
        grid: true,
    };
    scene.restore(objects.clone(), canvas).unwrap();
    assert_eq!(scene.objects(), objects);
    assert_eq!(scene.canvas(), canvas);
    assert!(scene.revision() > revision);
    assert!(scene.laser().is_none());
    assert!(!scene.undo());
    let before = scene.clone();
    assert!(
        scene
            .restore(vec![stroke(1, 0.2), stroke(1, 0.3)], Canvas::default())
            .is_err()
    );
    assert_eq!(scene, before);
    assert!(
        scene
            .restore(vec![stroke(2, f32::NAN)], Canvas::default())
            .is_err()
    );
    assert_eq!(scene, before);
}

fn stroke(id: u64, x: f32) -> InkObject {
    InkObject {
        id,
        tool: Tool::Freehand,
        style: InkStyle {
            rgba: [255, 0, 0, 255],
            width: 0.005,
        },
        points: vec![Point { x, y: 0.5 }],
        text: None,
        image_data: None,
        transform: Transform::default(),
    }
}

fn shape(tool: Tool) -> InkObject {
    let mut object = stroke(1, 0.2);
    object.tool = tool;
    object.points.push(Point { x: 0.6, y: 0.8 });
    object
}

fn png_data(width: u32, height: u32, pixels: &[u8]) -> String {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    STANDARD.encode(bytes)
}

#[test]
fn legacy_json_defaults_and_new_tool_geometry_are_validated() {
    let object:InkObject=serde_json::from_str(r#"{"id":7,"tool":"freehand","style":{"rgba":[1,2,3,255],"width":0.01},"points":[{"x":0.2,"y":0.3}]}"#).unwrap();
    assert_eq!(object.transform, Transform::default());
    Scene::default().commit(object).unwrap();
    for tool in [
        Tool::Line,
        Tool::Circle,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Arrow,
    ] {
        let object = shape(tool);
        Scene::default().commit(object.clone()).unwrap();
        let mut invalid = object;
        invalid.points.push(Point { x: 0.8, y: 0.5 });
        assert!(Scene::default().commit(invalid).is_err());
    }
    for tool in [
        Tool::Freehand,
        Tool::Highlighter,
        Tool::Polyline,
        Tool::Polygon,
    ] {
        let mut object = shape(tool);
        if tool == Tool::Polygon {
            assert!(Scene::default().commit(object.clone()).is_err());
        }
        object.points.push(Point { x: 0.8, y: 0.5 });
        let mut scene = Scene::default();
        scene.commit(object.clone()).unwrap();
        assert_eq!(scene.objects()[0].points, object.points);
    }
}

#[test]
fn validates_utf8_text_payload_and_bounded_png_decode() {
    let mut text = shape(Tool::Text);
    for payload in [
        None,
        Some("".into()),
        Some(" ".into()),
        Some("é".repeat(1025)),
        Some("bad\u{0}text".into()),
        Some("line\n".repeat(8)),
        Some(format!("x{}", "\n".repeat(2000))),
    ] {
        text.text = payload;
        assert!(Scene::default().commit(text.clone()).is_err());
    }
    text.text = Some("é".repeat(1024));
    Scene::default().commit(text.clone()).unwrap();
    text.text = Some(["line"; 8].join("\n"));
    Scene::default().commit(text).unwrap();
    let good = png_data(1, 1, &[20, 40, 60, 255]);
    validate_image(&good).unwrap();
    for bad in [
        "not png".into(),
        format!("data:image/png;base64,{good}"),
        STANDARD.encode(b"\x89PNG\r\n\x1a\nbroken"),
        "A".repeat(440_000),
    ] {
        assert!(validate_image(&bad).is_err());
    }
    assert!(validate_image(&png_data(1281, 1, &vec![0; 1281 * 4])).is_err());
    validate_image(&png_data(1280, 1, &vec![0; 1280 * 4])).unwrap();
    let mut image = shape(Tool::Image);
    assert!(Scene::default().commit(image.clone()).is_err());
    image.image_data = Some(good);
    Scene::default().commit(image).unwrap();
}

#[test]
fn affine_validation_rejects_nonfinite_singular_and_out_of_bound_values() {
    for transform in [
        Transform {
            a: f32::NAN,
            ..Default::default()
        },
        Transform {
            b: f32::INFINITY,
            ..Default::default()
        },
        Transform {
            a: 16.01,
            ..Default::default()
        },
        Transform {
            tx: 4.01,
            ..Default::default()
        },
        Transform {
            a: 0.0,
            ..Default::default()
        },
        Transform {
            a: 0.00001,
            ..Default::default()
        },
    ] {
        let mut object = shape(Tool::Line);
        object.transform = transform;
        assert!(Scene::default().commit(object).is_err());
    }
    Transform {
        a: -1.0,
        tx: 1.0,
        ..Default::default()
    }
    .validate()
    .unwrap();
    Transform {
        a: 0.0001,
        ..Default::default()
    }
    .validate()
    .unwrap();
    Transform {
        a: 0.01,
        d: 0.01,
        ..Default::default()
    }
    .validate()
    .unwrap();
    Transform {
        a: 16.0,
        d: 16.0,
        tx: 4.0,
        ty: -4.0,
        ..Default::default()
    }
    .validate()
    .unwrap();
}

#[test]
fn selection_retains_local_frame_across_rotation_and_reselection() {
    let aspect = 2.0;
    let angle = 0.6f32;
    let transform = Transform {
        a: angle.cos(),
        b: angle.sin() * aspect,
        c: -angle.sin() / aspect,
        d: angle.cos(),
        tx: 0.2,
        ty: -0.1,
    };
    for tool in [Tool::Rectangle, Tool::Image, Tool::Ellipse, Tool::Circle] {
        let mut object = shape(tool);
        let local = geometry_bounds(&object, aspect);
        object.transform = transform;
        let selected = selection(&object, aspect);
        assert_eq!(selected.local_bounds, local);
        assert_ne!(selected.bounds, local);
        let corner = transform.apply(Point {
            x: local.left,
            y: local.top,
        });
        let serialized_corner = selected.transform.apply(Point {
            x: selected.local_bounds.left,
            y: selected.local_bounds.top,
        });
        assert_eq!(corner, serialized_corner);
        assert_eq!(selected, selection(&object, aspect));
        object.transform = Transform::default();
        assert_eq!(selection(&object, aspect).local_bounds, local);
    }
}

#[test]
fn absolute_transforms_are_reversible_and_selection_uses_transformed_vertices() {
    let mut scene = Scene::default();
    let object = shape(Tool::Line);
    let points = object.points.clone();
    scene.commit(object).unwrap();
    let transform = Transform {
        a: 0.0,
        b: 1.0,
        c: -1.0,
        d: 0.0,
        tx: 1.0,
        ty: 0.0,
    };
    let selected = scene.transform_object(1, transform, 1.0).unwrap();
    assert!((selected.bounds.left - 0.2).abs() < 0.00001);
    assert!((selected.bounds.right - 0.5).abs() < 0.00001);
    assert!(
        scene
            .select(Point { x: 0.35, y: 0.4 }, 1.0)
            .unwrap()
            .is_some()
    );
    assert!(
        scene
            .select(Point { x: 0.2, y: 0.2 }, 1.0)
            .unwrap()
            .is_none()
    );
    scene.transform_object(1, transform, 1.0).unwrap();
    assert_eq!(scene.objects()[0].transform, transform);
    assert_eq!(scene.objects()[0].points, points);
    scene.undo();
    assert_eq!(scene.objects()[0].transform, Transform::default());
    scene.redo();
    assert_eq!(scene.objects()[0].transform, transform);
    let revision = scene.revision();
    assert!(
        scene
            .transform_object(
                1,
                Transform {
                    a: 0.0,
                    ..Default::default()
                },
                1.0
            )
            .is_err()
    );
    assert_eq!(scene.revision(), revision);
}

#[test]
fn topmost_hit_and_physical_circle_bounds_follow_geometry() {
    let mut scene = Scene::default();
    scene.commit(shape(Tool::Line)).unwrap();
    let mut above = shape(Tool::Line);
    above.id = 2;
    scene.commit(above).unwrap();
    assert_eq!(
        scene
            .select(Point { x: 0.4, y: 0.65 }, 2.0)
            .unwrap()
            .unwrap()
            .id,
        2
    );
    let circle = shape(Tool::Circle);
    let b = geometry_bounds(&circle, 2.0);
    assert!(((b.right - b.left) * 2.0 - (b.bottom - b.top)).abs() < 0.00001);
    let mut scene = Scene::default();
    scene.commit(circle).unwrap();
    assert!(
        scene
            .select(
                Point {
                    x: (b.left + b.right) / 2.0,
                    y: b.top
                },
                2.0
            )
            .unwrap()
            .is_some()
    );
    assert!(
        scene
            .select(
                Point {
                    x: (b.left + b.right) / 2.0,
                    y: (b.top + b.bottom) / 2.0
                },
                2.0
            )
            .unwrap()
            .is_none()
    );
    let mut arrow = shape(Tool::Arrow);
    arrow.points = vec![Point { x: 0.4, y: 0.5 }, Point { x: 0.41, y: 0.5 }];
    arrow.style.width = 0.08;
    let head = arrow_heads(&arrow, 2.0)[0];
    let selected = selection(&arrow, 2.0);
    let bounds = selected.bounds;
    assert!(bounds.left < 0.4 && bounds.bottom > 0.5);
    assert_eq!(selected.local_bounds, bounds);
    let mut scene = Scene::default();
    scene.commit(arrow).unwrap();
    assert!(scene.select(head, 2.0).unwrap().is_some());
}

#[test]
fn canvas_clear_undo_redo_and_session_reset_include_background_but_not_laser() {
    let mut scene = Scene::default();
    let canvas = Canvas {
        blackboard: true,
        grid: true,
    };
    scene.set_canvas(canvas).unwrap();
    scene
        .set_laser(true, Point { x: 0.5, y: 0.5 }, [255, 0, 0, 255])
        .unwrap();
    scene.clear().unwrap();
    assert_eq!(scene.canvas(), Canvas::default());
    assert!(scene.laser().is_none());
    scene.undo();
    assert_eq!(scene.canvas(), canvas);
    assert!(scene.laser().is_none());
    scene.redo();
    assert_eq!(scene.canvas(), Canvas::default());
    scene.undo();
    scene.reset_session();
    assert_eq!(scene.canvas(), Canvas::default());
    assert!(!scene.undo() && !scene.redo());
    scene
        .set_laser(true, Point { x: 0.5, y: 0.5 }, [255, 0, 0, 255])
        .unwrap();
    let revision = scene.revision();
    scene.laser.as_mut().unwrap().expires = Instant::now() - Duration::from_millis(1);
    scene.tick();
    assert!(scene.laser().is_none());
    assert_eq!(scene.revision(), revision);
    assert!(!scene.undo());
}

#[test]
fn image_payloads_count_toward_scene_and_history_memory_budgets() {
    let mut seed = 12345678u32;
    let pixels: Vec<u8> = (0..256 * 256 * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let data = png_data(256, 256, &pixels);
    let mut scene = Scene::default();
    let mut rejected = false;
    for id in 1..40 {
        let mut image = shape(Tool::Image);
        image.id = id;
        image.image_data = Some(data.clone());
        if scene.commit(image).is_err() {
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    assert!(scene.objects().len() < 30);
    assert!(estimated_bytes(scene.objects()) <= MAX_SCENE_BYTES);
    assert!(
        scene
            .undo
            .iter()
            .map(|s| estimated_bytes(&s.objects) + size_of::<Canvas>())
            .sum::<usize>()
            <= MAX_HISTORY_BYTES
    );
    scene.clear().unwrap();
    assert!(scene.undo());
    assert!(!scene.objects().is_empty());
}

#[test]
fn undo_redo_and_clear_are_reversible() {
    let mut scene = Scene::default();
    scene.commit(stroke(1, 0.2)).unwrap();
    scene.commit(stroke(2, 0.4)).unwrap();
    assert!(scene.undo());
    assert_eq!(
        scene.objects().iter().map(|o| o.id).collect::<Vec<_>>(),
        [1]
    );
    assert!(scene.redo());
    assert_eq!(scene.objects().len(), 2);

    scene.clear().unwrap();
    assert!(scene.objects().is_empty());
    assert!(scene.undo());
    assert_eq!(scene.objects().len(), 2);
}

#[test]
fn session_reset_discards_previous_ink_and_history() {
    let mut scene = Scene::default();
    scene.commit(stroke(1, 0.2)).unwrap();
    scene.commit(stroke(2, 0.4)).unwrap();
    assert!(scene.undo());
    let revision = scene.revision();
    scene.reset_session();
    assert!(scene.objects().is_empty());
    assert!(!scene.undo());
    assert!(!scene.redo());
    assert!(scene.revision() > revision);
}

#[test]
fn a_new_commit_after_undo_discards_redo() {
    let mut scene = Scene::default();
    scene.commit(stroke(1, 0.2)).unwrap();
    scene.undo();
    scene.commit(stroke(2, 0.4)).unwrap();
    assert!(!scene.redo());
}

#[test]
fn rejects_nonfinite_and_out_of_source_points() {
    for point in [
        Point {
            x: f32::NAN,
            y: 0.5,
        },
        Point { x: 1.01, y: 0.5 },
    ] {
        let mut scene = Scene::default();
        let mut object = stroke(1, 0.2);
        object.points[0] = point;
        assert!(scene.commit(object).is_err());
        assert!(scene.objects().is_empty());
    }
}

#[test]
fn rejects_duplicate_ids_and_shape_with_missing_endpoint() {
    let mut scene = Scene::default();
    scene.commit(stroke(1, 0.2)).unwrap();
    assert!(scene.commit(stroke(1, 0.4)).is_err());

    let mut shape = stroke(2, 0.2);
    shape.tool = Tool::Rectangle;
    assert!(scene.commit(shape).is_err());
}
