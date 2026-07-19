//! Shared `key=value` / `key [unit]=value` text metadata parsing used by
//! both [`super::prusa_gcode`] and [`super::prusa_bgcode`] — PrusaSlicer
//! emits the same key set in both places, just delivered differently:
//!
//! - `prusa_gcode.rs` gets it via the `gcode` crate's parsed `Comment`
//!   values (already stripped of the `;`/`(...)` markers) from the
//!   comment-only footer block.
//! - `prusa_bgcode.rs` gets it via `binarygcode`'s metadata block, which is
//!   already raw `key=value` text with no comment markers at all.
//!
//! This module takes a plain iterator of bare `key=value` lines either way,
//! so both callers feed it the same shape once they've stripped their own
//! container's framing. See `SPEC.md` §4.
