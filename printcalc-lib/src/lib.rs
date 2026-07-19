//! Core library for calculating 3D print job costs from sliced-file output.
//!
//! `printcalc-lib` is the single home for all real logic — slice-file parsing,
//! cost calculation, profile CRUD, and job-tier conversion. `printcalc-cli` and
//! `printcalc-wasm` are thin frontends over [`ops`]; they should not need to
//! contain business logic of their own. See `SPEC.md` in the workspace root
//! for the full design.

pub mod model;
pub mod formats;
pub mod library;
pub mod ops;
