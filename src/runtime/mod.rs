//! Local runtime orchestration for the `line` binary.
//!
//! The entrypoint sees only [`run`]. Terminal state, persistence transactions,
//! path completion, recovery prompts, and SSH handoff remain internal seams.

mod controller;
mod path;
mod persistence;
mod session;
mod terminal;

use std::error::Error;

pub(crate) use controller::run;

type DynError = Box<dyn Error + Send + Sync>;

#[cfg(test)]
mod tests;
