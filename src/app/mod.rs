//! Application state and input handling for the Line TUI.
//!
//! This module deliberately knows nothing about persistence or the SSH
//! process. It turns terminal events into [`AppAction`] values; the binary
//! performs I/O and calls [`App::finish_save`] / [`App::finish_delete`] when
//! an operation has completed.

mod browse;
mod form;
mod helpers;
mod input;
mod mouse;
mod state;
mod types;

pub use form::FormState;
pub use mouse::{MouseRegions, MouseTarget};
pub use state::App;
pub use types::{
    AppAction, AuthDraft, DEFAULT_PORT, FormField, KeyChoice, KeySource, ProfileDraft, SaveMode,
    Screen,
};

#[cfg(test)]
mod tests;
