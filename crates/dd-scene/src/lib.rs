use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const MAX_OBJECTS: usize = 2_000;
pub const MAX_POINTS_PER_OBJECT: usize = 20_000;
pub const MAX_POINTS_PER_SCENE: usize = 200_000;
pub const MAX_SCENE_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY_STATES: usize = 32;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Freehand,
    Arrow,
    Rectangle,
    Ellipse,
    Circle,
    Line,
    Polygon,
    Polyline,
    Text,
    Image,
    Highlighter,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

impl Transform {
    pub fn validate(self) -> Result<(), String> {
        if [self.a, self.b, self.c, self.d]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 16.0)
            || [self.tx, self.ty]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 4.0)
            || (self.a * self.d - self.b * self.c).abs() < 0.0001
        {
            return Err("invalid affine transform".into());
        }
        Ok(())
    }
    pub fn apply(self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.tx,
            y: self.b * p.x + self.d * p.y + self.ty,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Canvas {
    pub blackboard: bool,
    pub grid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Bounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Selection {
    pub id: u64,
    pub bounds: Bounds,
    pub local_bounds: Bounds,
    pub transform: Transform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Laser {
    pub point: Point,
    pub rgba: [u8; 4],
    expires: Instant,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InkStyle {
    pub rgba: [u8; 4],
    pub width: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InkObject {
    pub id: u64,
    pub tool: Tool,
    pub style: InkStyle,
    pub points: Vec<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>,
    #[serde(default)]
    pub transform: Transform,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneFixture {
    pub objects: Vec<InkObject>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    objects: Vec<InkObject>,
    canvas: Canvas,
    laser: Option<Laser>,
    undo: Vec<SceneState>,
    redo: Vec<SceneState>,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct SceneState {
    objects: Vec<InkObject>,
    canvas: Canvas,
}

impl Scene {
    pub fn canvas(&self) -> Canvas {
        self.canvas
    }
    pub fn laser(&self) -> Option<&Laser> {
        self.laser.as_ref()
    }
    pub fn set_laser(&mut self, active: bool, point: Point, rgba: [u8; 4]) -> Result<(), String> {
        validate_point(point)?;
        self.laser = active.then(|| Laser {
            point,
            rgba,
            expires: Instant::now() + Duration::from_millis(300),
        });
        Ok(())
    }
    pub fn clear_laser(&mut self) {
        self.laser = None;
    }
    pub fn tick(&mut self) {
        if self
            .laser
            .as_ref()
            .is_some_and(|laser| Instant::now() >= laser.expires)
        {
            self.clear_laser();
        }
    }
    pub fn set_canvas(&mut self, canvas: Canvas) -> Result<(), String> {
        if self.canvas != canvas {
            self.save_undo()?;
            self.canvas = canvas;
            self.changed();
        }
        Ok(())
    }
    pub fn transform_object(
        &mut self,
        id: u64,
        transform: Transform,
        aspect: f32,
    ) -> Result<Selection, String> {
        transform.validate()?;
        validate_aspect(aspect)?;
        let index = self
            .objects
            .iter()
            .position(|o| o.id == id)
            .ok_or("object not found")?;
        if self.objects[index].transform == transform {
            return Ok(selection(&self.objects[index], aspect));
        }
        self.save_undo()?;
        self.objects[index].transform = transform;
        self.changed();
        Ok(selection(&self.objects[index], aspect))
    }
    pub fn select(&self, point: Point, aspect: f32) -> Result<Option<Selection>, String> {
        validate_point(point)?;
        validate_aspect(aspect)?;
        Ok(self
            .objects
            .iter()
            .rev()
            .find(|o| hit(o, point, aspect))
            .map(|o| selection(o, aspect)))
    }
    pub fn demo() -> Self {
        Self::from_fixture(SceneFixture {
            objects: vec![
                Self::object(
                    1,
                    Tool::Freehand,
                    [255, 0, 0, 255],
                    0.007,
                    &[(0.10, 0.72), (0.17, 0.65), (0.24, 0.68), (0.32, 0.52)],
                ),
                Self::object(
                    2,
                    Tool::Arrow,
                    [0, 90, 255, 255],
                    0.006,
                    &[(0.12, 0.20), (0.37, 0.37)],
                ),
                Self::object(
                    3,
                    Tool::Rectangle,
                    [255, 215, 0, 255],
                    0.006,
                    &[(0.43, 0.17), (0.68, 0.45)],
                ),
                Self::object(
                    4,
                    Tool::Ellipse,
                    [0, 180, 90, 255],
                    0.006,
                    &[(0.73, 0.21), (0.92, 0.46)],
                ),
            ],
        })
        .expect("built-in demo scene is valid")
    }

    pub fn from_fixture(fixture: SceneFixture) -> Result<Self, String> {
        validate_objects(&fixture.objects)?;
        if estimated_bytes(&fixture.objects).saturating_add(size_of::<Canvas>()) > MAX_SCENE_BYTES {
            return Err(format!(
                "scene size limit ({MAX_SCENE_BYTES} bytes) exceeded"
            ));
        }
        Ok(Self {
            objects: fixture.objects,
            ..Self::default()
        })
    }

    fn object(id: u64, tool: Tool, rgba: [u8; 4], width: f32, points: &[(f32, f32)]) -> InkObject {
        InkObject {
            id,
            tool,
            style: InkStyle { rgba, width },
            points: points.iter().map(|(x, y)| Point { x: *x, y: *y }).collect(),
            text: None,
            image_data: None,
            transform: Transform::default(),
        }
    }

    pub fn objects(&self) -> &[InkObject] {
        &self.objects
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn render_snapshot(&self) -> Self {
        Self {
            objects: self.objects.clone(),
            canvas: self.canvas,
            laser: self.laser.clone(),
            revision: self.revision,
            ..Default::default()
        }
    }

    pub fn restore(&mut self, objects: Vec<InkObject>, canvas: Canvas) -> Result<(), String> {
        let mut restored = Self::from_fixture(SceneFixture { objects })?;
        restored.canvas = canvas;
        restored.revision = self.revision;
        restored.changed();
        *self = restored;
        Ok(())
    }

    pub fn commit(&mut self, object: InkObject) -> Result<(), String> {
        validate_object(&object)?;
        if self.objects.iter().any(|existing| existing.id == object.id) {
            return Err(format!("object id {} already exists", object.id));
        }
        if self.objects.len() == MAX_OBJECTS {
            return Err(format!("scene object limit ({MAX_OBJECTS}) reached"));
        }
        let current_points: usize = self.objects.iter().map(|item| item.points.len()).sum();
        if current_points + object.points.len() > MAX_POINTS_PER_SCENE {
            return Err(format!(
                "scene point limit ({MAX_POINTS_PER_SCENE}) reached"
            ));
        }
        let next_bytes = estimated_bytes(&self.objects)
            .saturating_add(estimated_bytes(std::slice::from_ref(&object)))
            .saturating_add(size_of::<Canvas>());
        if next_bytes > MAX_SCENE_BYTES {
            return Err(format!(
                "scene size limit ({MAX_SCENE_BYTES} bytes) exceeded"
            ));
        }
        self.save_undo()?;
        self.objects.push(object);
        self.changed();
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.clear_laser();
        if self.objects.is_empty() && self.canvas == Canvas::default() {
            return Ok(());
        }
        self.save_undo()?;
        self.objects.clear();
        self.canvas = Canvas::default();
        self.changed();
        Ok(())
    }

    pub fn reset_session(&mut self) {
        self.objects.clear();
        self.canvas = Canvas::default();
        self.laser = None;
        self.undo.clear();
        self.redo.clear();
        self.changed();
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = self.replace_state(previous);
        push_bounded_history(&mut self.redo, current);
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.replace_state(next);
        push_bounded_history(&mut self.undo, current);
        self.changed();
        true
    }

    fn save_undo(&mut self) -> Result<(), String> {
        if estimated_bytes(&self.objects).saturating_add(size_of::<Canvas>()) > MAX_HISTORY_BYTES {
            return Err(format!(
                "current scene exceeds the undo history budget ({MAX_HISTORY_BYTES} bytes)"
            ));
        }
        push_bounded_history(
            &mut self.undo,
            SceneState {
                objects: self.objects.clone(),
                canvas: self.canvas,
            },
        );
        self.redo.clear();
        Ok(())
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
    fn replace_state(&mut self, state: SceneState) -> SceneState {
        SceneState {
            objects: std::mem::replace(&mut self.objects, state.objects),
            canvas: std::mem::replace(&mut self.canvas, state.canvas),
        }
    }
}

fn validate_objects(objects: &[InkObject]) -> Result<(), String> {
    if objects.len() > MAX_OBJECTS {
        return Err(format!("object limit ({MAX_OBJECTS}) exceeded"));
    }
    let mut total_points = 0usize;
    let mut ids = std::collections::HashSet::new();
    for object in objects {
        validate_object(object)?;
        if !ids.insert(object.id) {
            return Err(format!("object id {} already exists", object.id));
        }
        total_points = total_points.saturating_add(object.points.len());
        if total_points > MAX_POINTS_PER_SCENE {
            return Err(format!(
                "scene point limit ({MAX_POINTS_PER_SCENE}) exceeded"
            ));
        }
    }
    Ok(())
}

fn estimated_bytes(objects: &[InkObject]) -> usize {
    objects.iter().fold(0usize, |total, object| {
        total
            .saturating_add(size_of::<InkObject>())
            .saturating_add(object.points.len().saturating_mul(size_of::<Point>()))
            .saturating_add(object.text.as_ref().map_or(0, String::len))
            .saturating_add(object.image_data.as_ref().map_or(0, String::len))
    })
}

fn push_bounded_history(history: &mut Vec<SceneState>, state: SceneState) {
    history.push(state);
    while history.len() > MAX_HISTORY_STATES
        || history
            .iter()
            .map(|state| estimated_bytes(&state.objects) + size_of::<Canvas>())
            .sum::<usize>()
            > MAX_HISTORY_BYTES
    {
        history.remove(0);
    }
}

fn validate_object(object: &InkObject) -> Result<(), String> {
    object.transform.validate()?;
    if let Some(text) = &object.text
        && (text.trim().is_empty()
            || text.len() > 2048
            || text.split('\n').count() > 8
            || text.chars().any(|c| {
                (c.is_control() && c != '\n' && c != '\t') || matches!(c, '\u{fffe}' | '\u{ffff}')
            }))
    {
        return Err(
            "text must contain 1..=2048 UTF-8 bytes, at most 8 lines, without control characters"
                .into(),
        );
    }
    if object.tool == Tool::Text && object.text.is_none() {
        return Err("text payload missing".into());
    }
    if let Some(data) = &object.image_data {
        validate_image(data)?;
    }
    if object.tool == Tool::Image && object.image_data.is_none() {
        return Err("image payload missing".into());
    }
    if object.points.is_empty() || object.points.len() > MAX_POINTS_PER_OBJECT {
        return Err(format!(
            "object {} must contain 1..={MAX_POINTS_PER_OBJECT} points",
            object.id
        ));
    }
    if !object.style.width.is_finite() || !(0.0..=0.2).contains(&object.style.width) {
        return Err(format!("object {} has invalid width", object.id));
    }
    if object.style.width == 0.0 {
        return Err(format!("object {} has zero width", object.id));
    }
    let valid_point_count = match object.tool {
        Tool::Freehand | Tool::Highlighter => !object.points.is_empty(),
        Tool::Polyline => object.points.len() >= 2,
        Tool::Polygon => object.points.len() >= 3,
        _ => object.points.len() == 2,
    };
    if !valid_point_count {
        return Err(format!(
            "object {} has an invalid point count for its tool",
            object.id
        ));
    }
    for point in &object.points {
        if validate_point(*point).is_err() {
            return Err(format!(
                "object {} has an invalid normalized point",
                object.id
            ));
        }
    }
    Ok(())
}
pub fn validate_image(data: &str) -> Result<(), String> {
    if data.len() > (320 * 1024usize).div_ceil(3) * 4 {
        return Err("PNG exceeds 320 KiB".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|e| format!("invalid PNG base64: {e}"))?;
    if bytes.len() > 320 * 1024
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || STANDARD.encode(&bytes) != data
    {
        return Err("image must be canonical base64 PNG, at most 320 KiB".into());
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("invalid PNG: {e}"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 1280
        || info.height > 1280
        || info.animation_control.is_some()
        || reader.output_buffer_size() > 16 * 1024 * 1024
    {
        return Err("PNG dimensions/allocation exceed limits or PNG is animated".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("invalid PNG pixels: {e}"))?;
    reader
        .finish()
        .map_err(|e| format!("invalid PNG end: {e}"))?;
    Ok(())
}

fn validate_point(point: Point) -> Result<(), String> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !(0.0..=1.0).contains(&point.x)
        || !(0.0..=1.0).contains(&point.y)
    {
        Err("coordinates must be finite and normalized".into())
    } else {
        Ok(())
    }
}

fn validate_aspect(aspect: f32) -> Result<(), String> {
    if !aspect.is_finite() || !(0.01..=100.0).contains(&aspect) {
        return Err("invalid source aspect ratio".into());
    }
    Ok(())
}

/// Geometry before its affine transform, including physical-circle aspect correction.
pub fn geometry_bounds(object: &InkObject, aspect: f32) -> Bounds {
    let mut b = Bounds {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    for p in &object.points {
        b.left = b.left.min(p.x);
        b.right = b.right.max(p.x);
        b.top = b.top.min(p.y);
        b.bottom = b.bottom.max(p.y);
    }
    if object.tool == Tool::Circle {
        let diameter = ((b.right - b.left) * aspect).min(b.bottom - b.top);
        let cx = (b.left + b.right) / 2.0;
        let cy = (b.top + b.bottom) / 2.0;
        b = Bounds {
            left: cx - diameter / aspect / 2.0,
            right: cx + diameter / aspect / 2.0,
            top: cy - diameter / 2.0,
            bottom: cy + diameter / 2.0,
        };
    }
    b
}

fn arrow_heads(object: &InkObject, aspect: f32) -> [Point; 2] {
    let start = object.points[0];
    let end = object.points[1];
    let angle = (end.y - start.y).atan2((end.x - start.x) * aspect);
    let head = aspect.min(1.0) * (0.022f32).max(object.style.width * 3.0);
    [-0.55f32, 0.55f32].map(|delta| {
        let a = angle + std::f32::consts::PI + delta;
        Point {
            x: end.x + a.cos() * head / aspect,
            y: end.y + a.sin() * head,
        }
    })
}

fn selection(o: &InkObject, aspect: f32) -> Selection {
    let mut b = geometry_bounds(o, aspect);
    if o.tool == Tool::Arrow {
        for p in arrow_heads(o, aspect) {
            b.left = b.left.min(p.x);
            b.right = b.right.max(p.x);
            b.top = b.top.min(p.y);
            b.bottom = b.bottom.max(p.y);
        }
    }
    if matches!(o.tool, Tool::Ellipse | Tool::Circle) {
        let center = o.transform.apply(Point {
            x: (b.left + b.right) / 2.0,
            y: (b.top + b.bottom) / 2.0,
        });
        let rx = (b.right - b.left) / 2.0;
        let ry = (b.bottom - b.top) / 2.0;
        let dx = (o.transform.a * rx).hypot(o.transform.c * ry);
        let dy = (o.transform.b * rx).hypot(o.transform.d * ry);
        return Selection {
            id: o.id,
            transform: o.transform,
            local_bounds: b,
            bounds: Bounds {
                left: center.x - dx,
                right: center.x + dx,
                top: center.y - dy,
                bottom: center.y + dy,
            },
        };
    }
    let mut out = Bounds {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    let corners = [
        Point {
            x: b.left,
            y: b.top,
        },
        Point {
            x: b.right,
            y: b.top,
        },
        Point {
            x: b.left,
            y: b.bottom,
        },
        Point {
            x: b.right,
            y: b.bottom,
        },
    ];
    let vertices = if matches!(o.tool, Tool::Rectangle | Tool::Text | Tool::Image) {
        &corners[..]
    } else {
        &o.points[..]
    };
    for p in vertices {
        let p = o.transform.apply(*p);
        out.left = out.left.min(p.x);
        out.right = out.right.max(p.x);
        out.top = out.top.min(p.y);
        out.bottom = out.bottom.max(p.y);
    }
    if o.tool == Tool::Arrow {
        for p in arrow_heads(o, aspect) {
            let p = o.transform.apply(p);
            out.left = out.left.min(p.x);
            out.right = out.right.max(p.x);
            out.top = out.top.min(p.y);
            out.bottom = out.bottom.max(p.y);
        }
    }
    Selection {
        id: o.id,
        bounds: out,
        local_bounds: b,
        transform: o.transform,
    }
}

fn hit(o: &InkObject, p: Point, aspect: f32) -> bool {
    let t = o.transform;
    let det = t.a * t.d - t.b * t.c;
    let p = Point {
        x: (t.d * (p.x - t.tx) - t.c * (p.y - t.ty)) / det,
        y: (-t.b * (p.x - t.tx) + t.a * (p.y - t.ty)) / det,
    };
    let b = if o.tool == Tool::Arrow {
        let mut b = geometry_bounds(o, aspect);
        for p in arrow_heads(o, aspect) {
            b.left = b.left.min(p.x);
            b.right = b.right.max(p.x);
            b.top = b.top.min(p.y);
            b.bottom = b.bottom.max(p.y);
        }
        b
    } else {
        geometry_bounds(o, aspect)
    };
    let tolerance = (0.012 + o.style.width / 2.0)
        / ((t.a * t.a + t.b * t.b)
            .sqrt()
            .min((t.c * t.c + t.d * t.d).sqrt())
            .max(0.01));
    if p.x < b.left - tolerance / aspect
        || p.x > b.right + tolerance / aspect
        || p.y < b.top - tolerance
        || p.y > b.bottom + tolerance
    {
        return false;
    }
    let distance = |a: Point, z: Point| {
        let dx = (z.x - a.x) * aspect;
        let dy = z.y - a.y;
        let px = (p.x - a.x) * aspect;
        let py = p.y - a.y;
        let u = ((px * dx + py * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
        ((px - u * dx).powi(2) + (py - u * dy).powi(2)).sqrt()
    };
    match o.tool {
        Tool::Text | Tool::Image => true,
        Tool::Rectangle => {
            (p.x - b.left).abs() * aspect <= tolerance
                || (p.x - b.right).abs() * aspect <= tolerance
                || (p.y - b.top).abs() <= tolerance
                || (p.y - b.bottom).abs() <= tolerance
        }
        Tool::Circle | Tool::Ellipse => {
            let rx = (b.right - b.left) * aspect / 2.0;
            let ry = (b.bottom - b.top) / 2.0;
            let x = (p.x - (b.left + b.right) / 2.0) * aspect;
            let y = p.y - (b.top + b.bottom) / 2.0;
            if rx <= 1e-6 || ry <= 1e-6 {
                distance(o.points[0], o.points[1]) <= tolerance
            } else {
                ((x / rx).hypot(y / ry) - 1.0).abs() * rx.min(ry) <= tolerance
            }
        }
        _ => {
            o.points
                .windows(2)
                .any(|s| distance(s[0], s[1]) <= tolerance)
                || (o.points.len() == 1 && distance(o.points[0], o.points[0]) <= tolerance)
                || (o.tool == Tool::Polygon
                    && distance(*o.points.last().unwrap(), o.points[0]) <= tolerance)
                || (o.tool == Tool::Arrow
                    && arrow_heads(o, aspect)
                        .iter()
                        .any(|head| distance(o.points[1], *head) <= tolerance))
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/scene.rs"]
mod tests;
