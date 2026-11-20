//! Sending the focused window to the back: each press sends it behind the windows it covers
//! and focuses a window over where it was (`sends`), by raising the windows over it on macOS
//! (`raising`), and a press held long enough puts it back as it's let go (`put_back`).
//! `press` covers pressing and letting go, and `support` what they share.

mod press;
mod put_back;
mod raising;
mod sends;
mod support;
