//! `SliceParser` trait, format detection, and per-slicer parsers.
//! See `SPEC.md` §2, §4.

pub mod slice_summary;

#[cfg(feature = "bambu-3mf")]
pub mod bambu_3mf;

#[cfg(feature = "prusa-gcode")]
pub mod prusa_gcode;

#[cfg(feature = "prusa-sl1")]
pub mod prusa_sl1;

#[cfg(feature = "prusa-bgcode")]
pub mod prusa_bgcode;

#[cfg(any(feature = "prusa-gcode", feature = "prusa-bgcode"))]
mod prusa_metadata;

// TODO: SliceParser trait, detect_format() — see SPEC.md §2, §4.
