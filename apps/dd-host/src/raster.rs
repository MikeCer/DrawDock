use dd_scene::{InkObject, Scene, Tool, geometry_bounds};
use resvg::{tiny_skia, usvg};
use std::{fmt::Write as _, sync::OnceLock};

fn options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options {
            font_family: "Segoe UI".into(),
            ..Default::default()
        };
        options.fontdb_mut().load_system_fonts();
        options.fontdb_mut().set_sans_serif_family("Segoe UI");
        // Only our generated, validated PNG data URLs are accepted. Never access files.
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        options
    })
}

pub fn render(scene: &Scene, width: usize, height: usize, pixels: &mut [u8]) -> Result<(), String> {
    let bytes = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= 512 * 1024 * 1024)
        .ok_or("overlay raster exceeds size limit")?;
    if width == 0 || height == 0 || pixels.len() != bytes {
        return Err("invalid overlay raster dimensions/buffer".into());
    }
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    if scene.canvas().blackboard {
        svg.push_str(r#"<rect width="100%" height="100%" fill="black"/>"#);
    }
    if scene.canvas().grid {
        svg.push_str(r##"<defs><pattern id="grid" width="32" height="32" patternUnits="userSpaceOnUse"><path d="M32 0 H0 V32" fill="none" stroke="#8a94a6" stroke-opacity="0.5" stroke-width="1"/></pattern></defs><rect width="100%" height="100%" fill="url(#grid)"/>"##);
    }
    for object in scene.objects() {
        append_object(&mut svg, object, width as f32, height as f32)?;
    }
    if let Some(laser) = scene.laser() {
        let [r, g, b, a] = laser.rgba;
        write!(svg, r#"<circle cx="{}" cy="{}" r="6" fill="rgb({r},{g},{b})" fill-opacity="{}" stroke="white" stroke-width="1"/>"#,
            laser.point.x*width as f32, laser.point.y*height as f32, a as f32/255.0).unwrap();
    }
    svg.push_str("</svg>");
    let tree = usvg::Tree::from_str(&svg, options())
        .map_err(|e| format!("parse generated overlay SVG: {e}"))?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width as u32, height as u32).ok_or("allocate overlay raster")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    for (out, rgba) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixmap.data().as_chunks::<4>().0.iter())
    {
        out.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn append_object(svg: &mut String, o: &InkObject, w: f32, h: f32) -> Result<(), String> {
    let t = o.transform;
    let [r, g, b, a] = o.style.rgba;
    let alpha = if o.tool == Tool::Highlighter {
        (a as f32 / 255.0).min(0.35)
    } else {
        a as f32 / 255.0
    };
    let stroke = (o.style.width * w.min(h)).max(1.0);
    write!(svg,r#"<g transform="matrix({} {} {} {} {} {})" opacity="{alpha}" fill="none" stroke="rgb({r},{g},{b})" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round">"#,
        t.a,t.b*h/w,t.c*w/h,t.d,t.tx*w,t.ty*h).unwrap();
    let bounds = geometry_bounds(o, w / h);
    let (left, top, right, bottom) = (
        bounds.left * w,
        bounds.top * h,
        bounds.right * w,
        bounds.bottom * h,
    );
    match o.tool {
        Tool::Rectangle | Tool::Ellipse | Tool::Circle
            if right - left < 1.0 || bottom - top < 1.0 =>
        {
            write!(svg, r#"<path d="M{left} {top} L{right} {bottom}"/>"#).unwrap();
        }
        Tool::Rectangle => {
            write!(
                svg,
                r#"<rect x="{left}" y="{top}" width="{}" height="{}"/>"#,
                right - left,
                bottom - top
            )
            .unwrap();
        }

        Tool::Ellipse | Tool::Circle => {
            write!(
                svg,
                r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
                (left + right) / 2.0,
                (top + bottom) / 2.0,
                (right - left) / 2.0,
                (bottom - top) / 2.0
            )
            .unwrap();
        }
        Tool::Text => {
            let text = o
                .text
                .as_deref()
                .ok_or("text payload missing while rendering")?;
            if options().fontdb.is_empty() {
                return Err("no Windows system fonts available for overlay text".into());
            }
            let size = o.style.width * w.min(h);
            write!(svg,r#"<svg x="{left}" y="{top}" width="{}" height="{}" overflow="hidden"><text font-family="Segoe UI" font-size="{size}" fill="rgb({r},{g},{b})" stroke="none" xml:space="preserve">"#,right-left,bottom-top).unwrap();
            for (index, line) in text.lines().enumerate() {
                write!(
                    svg,
                    r#"<tspan x="0" y="{}">{}</tspan>"#,
                    size * (1.0 + index as f32 * 1.2),
                    escape(line)
                )
                .unwrap();
            }
            svg.push_str("</text></svg>");
        }
        Tool::Image => {
            let data = o
                .image_data
                .as_deref()
                .ok_or("image payload missing while rendering")?;
            write!(svg,r#"<image x="{left}" y="{top}" width="{}" height="{}" preserveAspectRatio="none" xlink:href="data:image/png;base64,{data}"/>"#,right-left,bottom-top).unwrap();
        }
        _ => {
            let first = o.points.first().ok_or("empty overlay path")?;
            let (x0, y0) = (first.x * w, first.y * h);
            if o.points.len() == 1 {
                write!(
                    svg,
                    r#"<circle cx="{x0}" cy="{y0}" r="{}" fill="rgb({r},{g},{b})" stroke="none"/>"#,
                    stroke / 2.0
                )
                .unwrap();
            } else {
                write!(svg, r#"<path d="M{x0} {y0}"#).unwrap();
                for p in &o.points[1..] {
                    write!(svg, " L{} {}", p.x * w, p.y * h).unwrap();
                }
                if o.tool == Tool::Polygon {
                    svg.push_str(" Z");
                }
                if o.tool == Tool::Arrow {
                    let p = o.points[1];
                    let (x, y) = (p.x * w, p.y * h);
                    let angle = (y - y0).atan2(x - x0);
                    let head = (w.min(h) * 0.022).max(stroke * 3.0);
                    for delta in [-0.55f32, 0.55f32] {
                        let a = angle + std::f32::consts::PI + delta;
                        write!(
                            svg,
                            " M{x} {y} L{} {}",
                            x + a.cos() * head,
                            y + a.sin() * head
                        )
                        .unwrap();
                    }
                }
                svg.push_str(r#""/>"#);
            }
        }
    }
    svg.push_str("</g>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use dd_scene::{Canvas, InkStyle, Point, Transform};

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
}
