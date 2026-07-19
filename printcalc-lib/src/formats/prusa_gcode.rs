//! PrusaSlicer FDM `.gcode` parser: plain text with a slicer-generated
//! comment footer (`; key = value` / `; key [unit] = value` lines). Shares
//! the key=value line parsing in [`super::prusa_metadata`] with
//! [`super::prusa_bgcode`], since PrusaSlicer emits the same metadata keys
//! in both formats. See `SPEC.md` §4.
