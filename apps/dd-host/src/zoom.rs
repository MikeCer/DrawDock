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
mod tests {
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
}
