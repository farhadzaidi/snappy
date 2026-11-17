//! `snappy --uninstall`: undo everything Snappy added to the system, through the same code
//! that adds it, and delete its files, leaving only the program itself for whatever installed
//! it to remove. The uninstall scripts in `tools/uninstall/` run this first.
//!
//! It quits the running daemon the way the menu's Quit does, so hidden windows come back and
//! Caps Lock is given back. When nothing runs but an earlier run left windows hidden or Caps
//! Lock changed, as a crash does, Snappy starts once to pick that up and quits. Then each
//! adapter removes what it added outside Snappy's folders, and the folders go.

mod folders;
mod messages;
mod run;
mod system;

pub use run::run;
