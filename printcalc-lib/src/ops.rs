//! Orchestration functions: `new_job`, `calc_job`, `bundle_job`, and
//! add/list/show for printer & filament profiles.
//!
//! Both `printcalc-cli` and `printcalc-wasm` call these directly rather than
//! reimplementing any of this logic themselves — see the "Principle" note
//! in `SPEC.md` §1.
