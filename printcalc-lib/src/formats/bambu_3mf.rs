//! Bambu Studio `.gcode.3mf` parser.
//!
//! Zip (3MF) containing per-plate embedded gcode
//! (`Metadata/plate_N.gcode`) plus `Metadata/slice_info.config` (XML, one
//! `<plate>` element per plate with `prediction`/`weight`/`<filament>`) and
//! `Metadata/project_settings.config` (JSON). Confirmed against
//! `Testfiles/single_plate.gcode.3mf` and `multi_plate.gcode.3mf`.
//! See `SPEC.md` §4.
