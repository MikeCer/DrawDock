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

pub fn laser_sprite(rgba: [u8; 4]) -> Result<Vec<u8>, String> {
    let mut pixmap = tiny_skia::Pixmap::new(16, 16).ok_or("allocate laser sprite")?;
    let circle = tiny_skia::PathBuilder::from_circle(8.0, 8.0, 6.0).ok_or("build laser circle")?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    pixmap.fill_path(
        &circle,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    paint.set_color_rgba8(255, 255, 255, 255);
    pixmap.stroke_path(
        &circle,
        &paint,
        &tiny_skia::Stroke::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    let mut pixels = pixmap.take();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(pixels)
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
#[path = "../tests/unit/raster.rs"]
mod tests;
