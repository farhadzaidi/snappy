//! What the adapters that work over the session bus share: why a call failed, a thread that
//! makes blocking calls for threads that can't, and threads that forward signals.

use std::sync::mpsc::{self, Sender};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};

use crate::journal;

/// Why a call didn't succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The bus or the service didn't answer, or what Snappy installed there isn't running.
    Unreachable(String),
    /// The service answered with an error, such as `gone` for a closed window.
    Refused(String),
    /// A reply that doesn't have the shape this version expects.
    Malformed(String),
}

impl CallError {
    /// Logs what went wrong for the daemon's output, once, where it happened. `from` names
    /// who replied.
    pub fn logged(self, from: &str) -> CallError {
        if let CallError::Malformed(detail) = &self {
            journal::warn(format_args!("unexpected reply from {from}: {detail}"));
        }
        self
    }
}

/// Work for the bus thread.
type Job = Box<dyn FnOnce(&Connection) + Send>;

/// A thread of its own for blocking calls on one connection: the engine thread drives an
/// async runtime, inside which zbus can't block, so it waits for this thread's answer instead.
#[derive(Clone)]
pub struct BusThread {
    jobs: Sender<Job>,
}

impl BusThread {
    pub fn spawn(conn: &Connection, name: &str) -> std::io::Result<BusThread> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let worker = conn.clone();
        std::thread::Builder::new().name(name.to_owned()).spawn(move || {
            for job in queue {
                job(&worker);
            }
        })?;
        Ok(BusThread { jobs })
    }

    /// Runs `job` on the bus thread and waits for its result.
    pub fn call<T: Send + 'static>(
        &self,
        job: impl FnOnce(&Connection) -> Result<T, CallError> + Send + 'static,
    ) -> Result<T, CallError> {
        let (answer, answered) = mpsc::channel();
        self.post(move |conn| {
            let _ = answer.send(job(conn));
        });
        answered
            .recv()
            .unwrap_or_else(|_| Err(CallError::Unreachable("the bus thread stopped".to_owned())))
    }

    /// Runs `job` on the bus thread without waiting for it.
    pub fn post(&self, job: impl FnOnce(&Connection) + Send + 'static) {
        let _ = self.jobs.send(Box::new(job));
    }
}

/// Forwards each signal matching `rule` that `translate` keeps, in order, from a thread named
/// `name`, and then `lost` once the bus goes away. Blocking reads can't be interrupted, so the
/// thread ends with the process.
pub fn forward_signals<T, E>(
    conn: &Connection,
    rule: MatchRule<'static>,
    name: &str,
    sender: Sender<T>,
    lost: T,
    translate: impl Fn(&zbus::Message) -> Option<T> + Send + 'static,
) -> Result<(), E>
where
    T: Send + 'static,
    E: From<zbus::Error> + From<std::io::Error>,
{
    let messages = MessageIterator::for_match_rule(rule, conn, None)?;
    std::thread::Builder::new().name(name.to_owned()).spawn(move || {
        for message in messages {
            let Ok(message) = message else { break };
            if let Some(incoming) = translate(&message)
                && sender.send(incoming).is_err()
            {
                return;
            }
        }
        let _ = sender.send(lost);
    })?;
    Ok(())
}
