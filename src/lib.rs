//! # Line
//!
//! A lightweight, high-performance Linux TUI SSH connection manager written in Rust.
//!
//! Line provides a responsive terminal launcher for saving and opening SSH connections
//! without repeatedly typing hostnames, usernames, or passwords.
//!
//! ## Architecture Overview
//!
//! The application is decomposed into five decoupled modules:
//!
//! - [`app`]: Pure, state-driven logic and event handling. Contains the application
//!   state machine, form validation, and text editing without any I/O or rendering dependencies.
//! - [`ui`]: Terminal rendering powered by [Ratatui](https://crates.io/crates/ratatui).
//!   Provides responsive layouts for both narrow and wide displays, theme palettes, and
//!   deterministic mouse hit-testing regions.
//! - [`config`]: Persistent configuration storage rooted at `~/.line/` (or `$LINE_CONFIG_DIR`).
//!   Manages atomic JSON document updates, automatic `.bak` rotation, and content-addressed
//!   SSH key deduplication.
//! - [`ssh`]: System OpenSSH runner, AskPass credential injector, PTY lifecycle management,
//!   and host key verification diagnostics.
//!
//! ## Credential & Security Model
//!
//! - State is stored in a private directory (`~/.line/`, mode `0700`).
//! - Configuration files and private keys are strictly permissions-checked to mode `0600`.
//! - Saved passwords are never passed as command-line arguments. Instead, they are passed
//!   via OpenSSH's standard `SSH_ASKPASS` protocol using an ephemeral per-session token.

pub mod app;
pub mod config;
pub mod ssh;
pub mod ui;
