//! Subcommand modules: one per top-level command.
//!
//! [`job`] is the default path — costing a sliced file or a saved job —
//! and is what runs when no subcommand is given. [`printer`] and
//! [`filament`] manage the profile library; [`config`] manages the
//! global defaults.
//!
//! The `clap` doc comments in these modules are the source of
//! `--help` text, so they are written for the terminal rather than for
//! rustdoc.

pub mod config;
pub mod filament;
pub mod job;
pub mod printer;
