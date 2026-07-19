//! Profile library load/save (`~/.config/printcalc/{printers,filaments}/*.toml`)
//! and global config (`~/.config/printcalc/config.toml`). See `SPEC.md` §6.
//!
//! Filesystem-only — not available to `printcalc-wasm`, which has no
//! filesystem and instead receives embedded profiles directly.
