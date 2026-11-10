//! Local, title-free diagnostics: drop outcomes, workspace lifecycle events, and switch
//! timings.

use std::collections::VecDeque;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use snappy_core::bounds::BoundsStep;
use snappy_core::geometry::{Point, Rect};

use crate::store::{append_log, write_state};

const DROPS_KEPT: usize = 20;
const LIFECYCLE_KEPT: usize = 30;
const TIMINGS_KEPT: usize = 120;

/// One modifier drag and how it ended.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropRecord {
    pub timestamp: f64,
    pub outcome: String,
    pub app: Option<String>,
    pub original: Option<Rect>,
    pub before_drop: Option<Rect>,
    pub requested: Option<Rect>,
    pub actual: Option<Rect>,
    pub pointer: Point,
    pub started_in_move_area: bool,
    pub cancellation: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub stage: String,
    pub frame: Rect,
    pub elapsed_ms: f64,
}

impl From<&BoundsStep> for Step {
    fn from(step: &BoundsStep) -> Self {
        Step {
            stage: step.stage.to_owned(),
            frame: step.frame,
            elapsed_ms: step.elapsed_ms,
        }
    }
}

/// One workspace switch, phase by phase.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTiming {
    pub queue_ms: f64,
    pub discovery_ms: f64,
    pub window_ms: f64,
    pub membership_ms: f64,
    pub settings_ms: f64,
    pub refresh_ms: f64,
    pub total_ms: f64,
    pub windows: usize,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleRecord {
    timestamp: f64,
    reason: String,
    paused: bool,
    active: Option<String>,
    windows: usize,
}

pub struct Diagnostics {
    folder: PathBuf,
    timings: VecDeque<SwitchTiming>,
    /// Settle steps of the last bounds change, for the next drop record.
    pub last_steps: Vec<BoundsStep>,
}

impl Diagnostics {
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            timings: VecDeque::new(),
            last_steps: Vec::new(),
        }
    }

    pub fn record_drop(&self, record: DropRecord) {
        append_log(&self.folder.join("drops.json"), record, DROPS_KEPT);
    }

    pub fn record_lifecycle(
        &self,
        timestamp: f64,
        reason: &str,
        paused: bool,
        active: Option<String>,
        windows: usize,
    ) {
        let record = LifecycleRecord {
            timestamp,
            reason: reason.to_owned(),
            paused,
            active,
            windows,
        };
        append_log(
            &self.folder.join("workspace-lifecycle.json"),
            record,
            LIFECYCLE_KEPT,
        );
    }

    pub fn record_switch(&mut self, timing: SwitchTiming) {
        self.timings.push_back(timing);
        while self.timings.len() > TIMINGS_KEPT {
            self.timings.pop_front();
        }
    }

    pub fn timings(&self) -> impl Iterator<Item = &SwitchTiming> {
        self.timings.iter()
    }

    /// Write switch timings at quit; nothing when there are none.
    pub fn export_timings(&self) {
        if !self.timings.is_empty() {
            let _ = write_state(&self.folder.join("workspace-performance.json"), &self.timings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_records_keep_the_last_thirty_events() {
        let folder = std::env::temp_dir().join(format!("snappy-lifecycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let diagnostics = Diagnostics::new(folder.clone());
        for event in 0..35 {
            let active = Some(format!("workspace-{event}"));
            diagnostics.record_lifecycle(
                f64::from(event),
                "nativeDesktopChanged",
                event % 2 == 0,
                active,
                3,
            );
        }

        let bytes = std::fs::read(folder.join("workspace-lifecycle.json")).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(records.len(), LIFECYCLE_KEPT);
        assert_eq!(LIFECYCLE_KEPT, 30);
        assert_eq!(
            records[0],
            serde_json::json!({
                "timestamp": 5.0,
                "reason": "nativeDesktopChanged",
                "paused": false,
                "active": "workspace-5",
                "windows": 3,
            })
        );
        assert_eq!(records[29]["active"], "workspace-34");
    }

    #[test]
    fn the_last_120_switch_timings_stay_in_memory_until_exported() {
        let folder = std::env::temp_dir().join(format!("snappy-timings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let file = folder.join("workspace-performance.json");
        let mut diagnostics = Diagnostics::new(folder);
        diagnostics.export_timings();
        assert!(!file.exists(), "nothing is written without switches");

        for switch in 0..130 {
            diagnostics.record_switch(SwitchTiming {
                windows: switch,
                succeeded: true,
                ..SwitchTiming::default()
            });
        }
        assert!(!file.exists());
        let kept: Vec<usize> = diagnostics.timings().map(|timing| timing.windows).collect();
        assert_eq!(kept, (10..130).collect::<Vec<_>>());

        diagnostics.export_timings();
        let written: Vec<SwitchTiming> = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(written.len(), 120);
        assert_eq!(written[0].windows, 10);
    }
}
