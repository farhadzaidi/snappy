use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum WindowsError {
    #[error("Snappy could not create its {what}: {source}")]
    Setup {
        what: &'static str,
        source: windows::core::Error,
    },
}

impl WindowsError {
    pub fn setup(what: &'static str) -> impl FnOnce(windows::core::Error) -> WindowsError {
        move |source| WindowsError::Setup { what, source }
    }
}
