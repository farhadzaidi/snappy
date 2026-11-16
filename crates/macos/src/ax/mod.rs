//! Safe wrappers over the Accessibility API.

mod element;
mod error;
pub mod names;
mod trust;
mod value;

pub use element::AxElement;
pub use error::AxError;
pub use trust::{TRUSTED_CHECK_OPTION_PROMPT, is_trusted, is_trusted_with_options};
