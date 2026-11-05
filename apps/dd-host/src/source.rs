use crate::{windows_overlay::Output, zoom::Viewport};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CLOCK_MARGIN_US: u64 = 33_334;
const MAX_PRESENTATIONS: usize = 128;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Presentation {
    pub epoch: u64,
    pub revision: u64,
    pub zoom: Viewport,
}

#[derive(Clone, Debug)]
struct PresentedInterval {
    after_us: u64,
    until_us: Option<u64>,
    state: Presentation,
}

#[derive(Clone, Debug)]
struct PresentationHistory {
    intervals: VecDeque<PresentedInterval>,
    wall_origin: SystemTime,
    monotonic_origin: Instant,
}

impl PresentationHistory {
    fn new() -> Self {
        Self {
            intervals: VecDeque::new(),
            wall_origin: SystemTime::now(),
            monotonic_origin: Instant::now(),
        }
    }

    fn now(&self) -> Result<u64, String> {
        let now = SystemTime::now();
        let expected = self.wall_origin + self.monotonic_origin.elapsed();
        let difference = now
            .duration_since(expected)
            .unwrap_or_else(|error| error.duration());
        if difference > Duration::from_millis(10) {
            return Err("system clock changed during video capture; reconnect required".into());
        }
        unix_time_us(now)
    }

    fn transition_at(&mut self, now: u64) {
        if let Some(last) = self.intervals.back_mut() {
            let until = now.saturating_sub(CLOCK_MARGIN_US);
            last.until_us = Some(last.until_us.map_or(until, |previous| previous.min(until)));
        }
    }

    fn record_at(&mut self, now: u64, state: Presentation) {
        if let Some(last) = self.intervals.back_mut()
            && last.until_us.is_none()
        {
            last.until_us = Some(now + CLOCK_MARGIN_US);
        }
        self.intervals.push_back(PresentedInterval {
            after_us: now + CLOCK_MARGIN_US,
            until_us: None,
            state,
        });
        while self.intervals.len() > MAX_PRESENTATIONS {
            self.intervals.pop_front();
        }
    }

    fn at(&self, capture_us: u64) -> Option<&Presentation> {
        self.intervals.iter().rev().find_map(|interval| {
            (capture_us >= interval.after_us
                && interval.until_us.is_none_or(|until| capture_us < until))
            .then_some(&interval.state)
        })
    }
}

fn unix_time_us(time: SystemTime) -> Result<u64, String> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros() as u64)
        .map_err(|_| {
            "Windows clock is before the Unix epoch; video timestamps are unavailable".into()
        })
}

#[derive(Clone, Debug)]
pub struct ZoomRequest {
    pub view: Viewport,
    pub reply: mpsc::SyncSender<Result<Viewport, String>>,
}

#[derive(Clone, Debug)]
pub struct SelectedSource {
    pub output: Output,
    pub epoch: u64,
    pub zoom: Viewport,
    pub pending_zoom: Option<ZoomRequest>,
    presentations: PresentationHistory,
}

pub type SharedSource = Arc<Mutex<SelectedSource>>;

impl SelectedSource {
    pub fn new(output: Output) -> Self {
        Self {
            output,
            epoch: 1,
            zoom: Viewport::default(),
            pending_zoom: None,
            presentations: PresentationHistory::new(),
        }
    }

    pub fn begin_presentation_change(&mut self) -> Result<(), String> {
        let now = unix_time_us(SystemTime::now())?;
        self.presentations.transition_at(now);
        Ok(())
    }

    pub fn needs_presentation(&self, revision: u64) -> bool {
        self.presentations.intervals.back().is_none_or(|last| {
            last.until_us.is_some()
                || last.state.epoch != self.epoch
                || last.state.revision != revision
                || last.state.zoom != self.zoom
        })
    }

    pub fn record_presentation(&mut self, revision: u64) -> Result<(), String> {
        let now = unix_time_us(SystemTime::now())?;
        self.presentations.record_at(
            now,
            Presentation {
                epoch: self.epoch,
                revision,
                zoom: self.zoom,
            },
        );
        Ok(())
    }

    pub fn captured_presentation(&self, capture_us: u64) -> Result<Option<&Presentation>, String> {
        let now = self.presentations.now()?;
        if capture_us > now + CLOCK_MARGIN_US || now.saturating_sub(capture_us) > 2_000_000 {
            return Err(
                "capture timestamp is stale or outside the host clock; reconnect required".into(),
            );
        }
        Ok(self.presentations.at(capture_us))
    }

    pub fn reset_presentations(&mut self) {
        self.presentations.intervals.clear();
    }

    pub fn restart_video_timeline(&mut self) {
        self.presentations = PresentationHistory::new();
    }

    pub fn metadata(&self) -> Value {
        json!({"source": self.output, "source_epoch": self.epoch, "zoom": self.zoom})
    }

    pub fn validate_epoch(&self, message: &Value) -> Result<(), String> {
        if message.get("source_epoch").and_then(Value::as_u64) != Some(self.epoch) {
            return Err("stale or missing source geometry epoch".into());
        }
        Ok(())
    }
}

pub fn apply_zoom(source: &SharedSource, view: Viewport) -> Result<Viewport, String> {
    let view = view.validate()?;
    let (reply, result) = mpsc::sync_channel(1);
    {
        let mut selected = source.lock().map_err(|_| "source lock poisoned")?;
        if selected.pending_zoom.is_some() {
            return Err("another zoom change is pending".into());
        }
        selected.pending_zoom = Some(ZoomRequest { view, reply });
    }
    result
        .recv_timeout(Duration::from_secs(2))
        .map_err(|e| format!("Windows zoom presenter did not acknowledge the change: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_discontinuities_fail_capture_but_a_new_video_session_can_recover() {
        let mut history = PresentationHistory::new();
        history.wall_origin -= Duration::from_secs(2);
        assert!(history.now().unwrap_err().contains("clock changed"));
        assert!(PresentationHistory::new().now().is_ok());
    }

    #[test]
    fn captured_revision_never_crosses_an_uncompleted_or_uncertain_presentation() {
        let mut history = PresentationHistory::new();
        let state = |revision| Presentation {
            epoch: 1,
            revision,
            zoom: Viewport::default(),
        };
        history.record_at(100_000, state(1));
        assert!(history.at(100_000).is_none());
        assert_eq!(history.at(140_000), Some(&state(1)));
        history.transition_at(200_000);
        assert!(history.at(180_000).is_none());
        history.record_at(240_000, state(2));
        assert!(history.at(250_000).is_none());
        assert_eq!(history.at(280_000), Some(&state(2)));
        assert_eq!(history.at(150_000), Some(&state(1)));
        for revision in 3..200 {
            history.record_at(300_000 + revision * 100_000, state(revision));
        }
        assert_eq!(history.intervals.len(), MAX_PRESENTATIONS);
        assert!(history.at(150_000).is_none());
    }

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
