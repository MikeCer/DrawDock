use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Freehand,
    Arrow,
    Rectangle,
    Ellipse,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct InkStyle {
    pub rgba: [u8; 4],
    pub width: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InkObject {
    pub id: u64,
    pub tool: Tool,
    pub style: InkStyle,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneFixture {
    pub objects: Vec<InkObject>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    objects: Vec<InkObject>,
    undo: Vec<Vec<InkObject>>,
    redo: Vec<Vec<InkObject>>,
    revision: u64,
}

impl Scene {
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
        if estimated_bytes(&fixture.objects) > MAX_SCENE_BYTES {
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
        }
    }

    pub fn objects(&self) -> &[InkObject] {
        &self.objects
    }

    pub fn revision(&self) -> u64 {
        self.revision
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
            .saturating_add(size_of::<InkObject>())
            .saturating_add(object.points.len() * size_of::<Point>());
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
        if self.objects.is_empty() {
            return Ok(());
        }
        self.save_undo()?;
        self.objects.clear();
        self.changed();
        Ok(())
    }

    pub fn reset_session(&mut self) {
        self.objects.clear();
        self.undo.clear();
        self.redo.clear();
        self.changed();
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.objects, previous);
        push_bounded_history(&mut self.redo, current);
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.objects, next);
        push_bounded_history(&mut self.undo, current);
        self.changed();
        true
    }

    fn save_undo(&mut self) -> Result<(), String> {
        if estimated_bytes(&self.objects) > MAX_HISTORY_BYTES {
            return Err(format!(
                "current scene exceeds the undo history budget ({MAX_HISTORY_BYTES} bytes)"
            ));
        }
        push_bounded_history(&mut self.undo, self.objects.clone());
        self.redo.clear();
        Ok(())
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

fn validate_objects(objects: &[InkObject]) -> Result<(), String> {
    if objects.len() > MAX_OBJECTS {
        return Err(format!("object limit ({MAX_OBJECTS}) exceeded"));
    }
    let mut total_points = 0usize;
    for object in objects {
        validate_object(object)?;
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
    })
}

fn push_bounded_history(history: &mut Vec<Vec<InkObject>>, state: Vec<InkObject>) {
    history.push(state);
    while history.len() > MAX_HISTORY_STATES
        || history
            .iter()
            .map(|objects| estimated_bytes(objects))
            .sum::<usize>()
            > MAX_HISTORY_BYTES
    {
        history.remove(0);
    }
}

fn validate_object(object: &InkObject) -> Result<(), String> {
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
        Tool::Freehand => object.points.len() >= 1,
        Tool::Arrow | Tool::Rectangle | Tool::Ellipse => object.points.len() == 2,
    };
    if !valid_point_count {
        return Err(format!(
            "object {} has an invalid point count for its tool",
            object.id
        ));
    }
    for point in &object.points {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !(0.0..=1.0).contains(&point.x)
            || !(0.0..=1.0).contains(&point.y)
        {
            return Err(format!(
                "object {} has an invalid normalized point",
                object.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(id: u64, x: f32) -> InkObject {
        InkObject {
            id,
            tool: Tool::Freehand,
            style: InkStyle {
                rgba: [255, 0, 0, 255],
                width: 0.005,
            },
            points: vec![Point { x, y: 0.5 }],
        }
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
}
