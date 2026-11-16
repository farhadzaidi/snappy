//! The window history section: undoing and redoing window moves.

use serde::{Deserialize, Serialize};

use crate::keys::KeyChord;

/// Undo and redo window-move shortcuts. Neither has a default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MoveHistory {
    pub undo: Option<KeyChord>,
    pub redo: Option<KeyChord>,
}

/// Which way through the window-move history a shortcut goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryStep {
    Undo,
    Redo,
}

impl HistoryStep {
    pub const ALL: [HistoryStep; 2] = [HistoryStep::Undo, HistoryStep::Redo];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            HistoryStep::Undo => "Undo window move",
            HistoryStep::Redo => "Redo window move",
        }
    }
}

impl MoveHistory {
    pub fn get(&self, step: HistoryStep) -> Option<&KeyChord> {
        match step {
            HistoryStep::Undo => self.undo.as_ref(),
            HistoryStep::Redo => self.redo.as_ref(),
        }
    }

    pub fn set(&mut self, step: HistoryStep, chord: Option<KeyChord>) {
        *self.slot(step) = chord;
    }

    pub(crate) fn slot(&mut self, step: HistoryStep) -> &mut Option<KeyChord> {
        match step {
            HistoryStep::Undo => &mut self.undo,
            HistoryStep::Redo => &mut self.redo,
        }
    }

    pub fn chords(&self) -> impl Iterator<Item = (HistoryStep, &KeyChord)> {
        HistoryStep::ALL
            .into_iter()
            .filter_map(|step| self.get(step).map(|chord| (step, chord)))
    }
}
