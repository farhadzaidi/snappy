//! State both threads read: window IDs, what the engine asked to observe, display
//! scales, and what's known about processes.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::identity::{Owner, Registry};
use crate::process;

/// What the engine asked to observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Monitoring {
    /// Move and size starts and ends, which stand in for primary-button presses.
    pub press: bool,
    /// The dragged window's movement and modifier changes.
    pub drag: bool,
}

impl Monitoring {
    pub fn presses(self) -> bool {
        self.press || self.drag
    }
}

/// What windows need from their process, read once per launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessFacts {
    pub app: String,
    pub elevated: bool,
}

#[derive(Default)]
pub struct Shared {
    pub registry: Mutex<Registry>,
    pub monitoring: Mutex<Monitoring>,
    /// Display IDs to scale factors, from the last display read.
    pub scales: Mutex<HashMap<String, f64>>,
    processes: Mutex<HashMap<Owner, ProcessFacts>>,
}

/// Locks a mutex, recovering from a panic on another thread.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub fn monitoring(&self) -> Monitoring {
        *lock(&self.monitoring)
    }

    pub fn scale(&self, display: &str) -> f64 {
        lock(&self.scales).get(display).copied().unwrap_or(1.0)
    }

    pub fn process(&self, owner: Owner) -> ProcessFacts {
        if let Some(facts) = lock(&self.processes).get(&owner) {
            return facts.clone();
        }
        let facts = ProcessFacts {
            app: process::executable(owner.pid).unwrap_or_else(|| format!("pid-{}", owner.pid)),
            elevated: process::is_elevated_above_self(owner.pid),
        };
        lock(&self.processes).insert(owner, facts.clone());
        facts
    }

    /// Drops what's cached about a process whose last window closed.
    pub fn forget_process(&self, pid: u32) {
        lock(&self.processes).retain(|owner, _| owner.pid != pid);
    }
}
