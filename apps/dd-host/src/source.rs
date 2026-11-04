use crate::windows_overlay::Output;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SelectedSource {
    pub output: Output,
    pub epoch: u64,
}

pub type SharedSource = Arc<Mutex<SelectedSource>>;

impl SelectedSource {
    pub fn new(output: Output) -> Self {
        Self { output, epoch: 1 }
    }

    pub fn metadata(&self) -> Value {
        json!({"source": self.output, "source_epoch": self.epoch})
    }

    pub fn validate_epoch(&self, message: &Value) -> Result<(), String> {
        if message.get("source_epoch").and_then(Value::as_u64) != Some(self.epoch) {
            return Err("stale or missing source geometry epoch".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_switch_rejects_queued_old_geometry_and_requires_current_epoch() {
        let output = Output {
            name: "test".into(),
            left: -100,
            top: 0,
            right: 100,
            bottom: 100,
            primary: false,
        };
        let mut source = SelectedSource::new(output);
        let queued = json!({"source_epoch": 1});
        assert!(source.validate_epoch(&queued).is_ok());
        source.epoch += 1;
        assert!(source.validate_epoch(&queued).is_err());
        assert!(source.validate_epoch(&json!({})).is_err());
        assert!(source.validate_epoch(&json!({"source_epoch": 3})).is_err());
        assert!(source.validate_epoch(&json!({"source_epoch": 2})).is_ok());
    }
}
