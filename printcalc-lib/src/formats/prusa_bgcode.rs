//! PrusaSlicer binary G-code (`.bgcode`) parser — the newer default output
//! format for MK4/MK4S/MINI+/XL firmware.
//!
//! Binary block container, magic `GCDE`. Confirmed against
//! `Testfiles/Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode`: a metadata block
//! holds the exact same `key=value` text schema as the plain `.gcode`
//! footer (`filament used [g]=`, `estimated printing time (normal mode)=`,
//! `printer_model=`, `filament_type=`, ...), ahead of compressed gcode
//! blocks. See [`super::prusa_metadata`] for the shared key=value parsing
//! used by both this and [`super::prusa_gcode`]. See `SPEC.md` §4.
