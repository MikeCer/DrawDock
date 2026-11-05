use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub scale: f32,
    pub left: f32,
    pub top: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scale: 1.0,
            left: 0.0,
            top: 0.0,
        }
    }
}

impl Viewport {
    pub fn validate(self) -> Result<Self, String> {
        let edge = 1.0 - 1.0 / self.scale;
        if !self.scale.is_finite()
            || !(1.0..=4.0).contains(&self.scale)
            || !self.left.is_finite()
            || !self.top.is_finite()
            || self.left < 0.0
            || self.top < 0.0
            || self.left > edge + 0.000001
            || self.top > edge + 0.000001
        {
            return Err(
                "zoom must be 100-400% with its viewport inside the selected output".into(),
            );
        }
        Ok(self)
    }

    pub fn for_pixels(self, width: i32, height: i32) -> Self {
        let edge = 1.0f64 - 1.0 / f64::from(self.scale);
        Self {
            left: (self.left * width as f32)
                .round()
                .min((edge * f64::from(width)).floor() as f32)
                / width as f32,
            top: (self.top * height as f32)
                .round()
                .min((edge * f64::from(height)).floor() as f32)
                / height as f32,
            ..self
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/zoom.rs"]
mod tests;
