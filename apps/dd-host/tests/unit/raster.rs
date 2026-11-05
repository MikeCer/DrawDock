use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use dd_scene::{Canvas, InkStyle, Point, Transform};

#[test]
#[ignore = "Measures 4K raster cost; run explicitly in debug and release profiles"]
fn performance_4k_laser_raster_baseline() {
    let mut scene = Scene::demo();
    scene
        .set_canvas(Canvas {
            blackboard: true,
            grid: true,
        })
        .unwrap();
    let mut pixels = vec![0; 3840 * 2160 * 4];
    let mut samples = Vec::new();
    for index in 0..23 {
        scene
            .set_laser(true, Point { x: 0.5, y: 0.5 }, [255, 0, 0, 255])
            .unwrap();
        let start = std::time::Instant::now();
        render(&scene, 3840, 2160, &mut pixels).unwrap();
        std::hint::black_box(&pixels);
        if index >= 3 {
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    assert_eq!(pixel(&pixels, 3840, 1920, 1080), [0, 0, 255, 255]);
    samples.sort_by(f64::total_cmp);
    println!(
        "4K grid+ink+laser raster-only, 20 samples: p50={:.2}ms p95={:.2}ms max={:.2}ms; debug_assertions={}",
        samples[9],
        samples[18],
        samples[19],
        cfg!(debug_assertions)
    );
}

fn object(tool: Tool) -> InkObject {
    InkObject {
        id: 1,
        tool,
        style: InkStyle {
            rgba: [255, 0, 0, 255],
            width: 0.04,
        },
        points: vec![Point { x: 0.2, y: 0.2 }, Point { x: 0.7, y: 0.7 }],
        ..Default::default()
    }
}

fn frame(scene: &Scene, w: usize, h: usize) -> Vec<u8> {
    let mut pixels = vec![0; w * h * 4];
    render(scene, w, h, &mut pixels).unwrap();
    pixels
}
fn pixel(pixels: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
    pixels[(y * w + x) * 4..(y * w + x + 1) * 4]
        .try_into()
        .unwrap()
}

#[test]
fn rasterizes_every_path_tool_and_physical_circle() {
    for tool in [
        Tool::Freehand,
        Tool::Highlighter,
        Tool::Line,
        Tool::Arrow,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Circle,
        Tool::Polyline,
        Tool::Polygon,
    ] {
        let mut item = object(tool);
        if matches!(
            tool,
            Tool::Freehand | Tool::Highlighter | Tool::Polyline | Tool::Polygon
        ) {
            item.points.push(Point { x: 0.8, y: 0.3 });
        }
        let mut scene = Scene::default();
        scene.commit(item).unwrap();
        let pixels = frame(&scene, 200, 100);
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p[2] > 0),
            "{tool:?}"
        );
        assert_eq!(pixel(&pixels, 200, 0, 0), [0; 4]);
        if tool == Tool::Circle {
            let occupied: Vec<(usize, usize)> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, p)| p[3] > 0)
                .map(|(i, _)| (i % 200, i / 200))
                .collect();
            let dx = occupied.iter().map(|p| p.0).max().unwrap()
                - occupied.iter().map(|p| p.0).min().unwrap();
            let dy = occupied.iter().map(|p| p.1).max().unwrap()
                - occupied.iter().map(|p| p.1).min().unwrap();
            assert!(dx.abs_diff(dy) <= 1, "circle distorted: {dx}x{dy}");
        }
    }
}

#[test]
fn affine_normalized_matrix_rotates_and_mirrors_on_non_square_output() {
    let mut item = object(Tool::Line);
    item.points = vec![Point { x: 0.2, y: 0.2 }, Point { x: 0.4, y: 0.2 }];
    item.transform = Transform {
        a: 0.0,
        b: 1.0,
        c: -1.0,
        d: 0.0,
        tx: 1.0,
        ty: 0.0,
    };
    let mut scene = Scene::default();
    scene.commit(item).unwrap();
    let pixels = frame(&scene, 200, 100);
    assert!(pixel(&pixels, 200, 160, 30)[3] > 0);
    assert_eq!(pixel(&pixels, 200, 60, 20), [0; 4]);
    scene
        .transform_object(
            1,
            Transform {
                a: -1.0,
                tx: 1.0,
                ..Default::default()
            },
            2.0,
        )
        .unwrap();
    let pixels = frame(&scene, 200, 100);
    assert!(pixel(&pixels, 200, 140, 20)[3] > 0);
    assert_eq!(pixel(&pixels, 200, 60, 20), [0; 4]);
    scene
        .transform_object(
            1,
            Transform {
                tx: 4.0,
                ..Default::default()
            },
            2.0,
        )
        .unwrap();
    assert!(frame(&scene, 200, 100).iter().all(|v| *v == 0));
}

#[test]
fn highlighter_is_composited_once_even_at_intersections_and_vertices() {
    let mut item = object(Tool::Highlighter);
    item.style.width = 0.1;
    item.points = vec![
        Point { x: 0.2, y: 0.5 },
        Point { x: 0.8, y: 0.5 },
        Point { x: 0.2, y: 0.5 },
    ];
    let mut scene = Scene::default();
    scene.commit(item).unwrap();
    let pixels = frame(&scene, 100, 100);
    let a = pixel(&pixels, 100, 50, 50)[3];
    assert!((85..=90).contains(&a), "{a}");
    assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] <= 90));
}

#[test]
fn renders_windows_font_text_and_validated_tablet_png_on_native_surface() {
    let mut text = object(Tool::Text);
    text.points = vec![Point { x: 0.1, y: 0.1 }, Point { x: 0.9, y: 0.9 }];
    text.style.width = 0.15;
    text.text = Some("Hello <>&\"'\nWorld".into());
    let mut scene = Scene::default();
    scene.commit(text).unwrap();
    let pixels = frame(&scene, 200, 100);
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
            > 100
    );
    assert_eq!(pixel(&pixels, 200, 0, 0), [0; 4]);

    let mut source = tiny_skia::Pixmap::new(1, 1).unwrap();
    source.fill(tiny_skia::Color::from_rgba8(20, 40, 60, 128));
    let mut image = object(Tool::Image);
    image.image_data = Some(STANDARD.encode(source.encode_png().unwrap()));
    image.transform.tx = 0.1;
    let mut scene = Scene::default();
    scene.commit(image).unwrap();
    let pixels = frame(&scene, 200, 100);
    let sample = pixel(&pixels, 200, 100, 40);
    assert_eq!(sample, [30, 20, 10, 128]);
    assert_eq!(pixel(&pixels, 200, 45, 40), [0; 4]);
}

#[test]
fn canvas_grid_clear_undo_and_transient_laser_render_on_native_surface() {
    let mut scene = Scene::default();
    scene
        .set_canvas(Canvas {
            blackboard: true,
            grid: false,
        })
        .unwrap();
    assert!(
        frame(&scene, 100, 100)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
    scene
        .set_canvas(Canvas {
            blackboard: false,
            grid: true,
        })
        .unwrap();
    let grid = frame(&scene, 100, 100);
    assert!(grid.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    assert_eq!(pixel(&grid, 100, 15, 15), [0; 4]);
    scene.clear().unwrap();
    assert!(frame(&scene, 100, 100).iter().all(|v| *v == 0));
    scene.undo();
    assert_eq!(frame(&scene, 100, 100), grid);
    scene.clear().unwrap();
    scene
        .set_laser(true, Point { x: 0.5, y: 0.5 }, [255, 0, 0, 255])
        .unwrap();
    assert_eq!(
        pixel(&frame(&scene, 100, 100), 100, 50, 50),
        [0, 0, 255, 255]
    );
    scene.clear_laser();
    assert!(frame(&scene, 100, 100).iter().all(|v| *v == 0));
}

#[test]
fn raster_dimension_failures_are_reported() {
    assert!(render(&Scene::default(), 0, 1, &mut []).is_err());
    assert!(render(&Scene::default(), 100, 100, &mut [0; 4]).is_err());
}
