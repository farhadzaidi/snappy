//! What the runner and the fixture app say to each other: one JSON object per line.

use serde::{Deserialize, Serialize};
use snappy_core::geometry::{Rect, Size};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    /// Open a window at `frame`'s origin with `frame`'s size as its content size.
    Open {
        id: String,
        frame: Rect,
        min_size: Option<Size>,
        /// Sizes change in steps of this much, like a terminal's character grid.
        grid: Option<Size>,
    },
    /// The window's outer frame, in top-left global points.
    Frame {
        id: String,
    },
    Focus {
        id: String,
    },
    Close {
        id: String,
    },
    /// Enter or leave the system's full screen.
    FullScreen {
        id: String,
        on: bool,
    },
    Quit,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub frame: Option<Rect>,
    pub error: Option<String>,
}

impl Reply {
    pub fn frame(frame: Rect) -> Self {
        Self {
            frame: Some(frame),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            frame: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_tagged_json() {
        let request = Request::Open {
            id: "a".into(),
            frame: Rect::new(10.0, 20.0, 300.0, 200.0),
            min_size: None,
            grid: Some(Size::new(7.0, 17.0)),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.starts_with(r#"{"op":"open","id":"a""#));
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
    }
}
