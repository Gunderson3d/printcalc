//! WASM bindings over `printcalc_lib::ops` for in-browser use. Thin
//! `wasm-bindgen` wrapper only — no business logic of its own. These
//! functions return a JSON `String` rather than a `JsValue`
//! (`JSON.parse()` it on the JS side) — same "SliceSummary/
//! CostBreakdown as JSON" contract, without needing `serde-wasm-bindgen`
//! (not otherwise a dependency anywhere in this workspace) just to hand
//! back a native JS object instead of a string.
//!
//! No filesystem here — the host page owns file input, profile
//! persistence (e.g. `localStorage`), and rendering; `calc_job` only
//! works on a job whose printer/filament refs and source are already
//! embedded (Full/Bundled tier), since there's no library to resolve a
//! `Linked` ref against.

use wasm_bindgen::prelude::*;

use printcalc_lib::model::job::JobSource;
use printcalc_lib::{builtin, formats, ops};

#[wasm_bindgen]
pub fn printcalc_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Built-in printer presets (see `printcalc_lib::builtin`), as a JSON
/// array of `PrinterProfile` — lets the host page offer them without a
/// filesystem to load a saved library from.
#[wasm_bindgen]
pub fn builtin_printers() -> Result<String, JsError> {
    serde_json::to_string(&builtin::printers()).map_err(|e| JsError::new(&e.to_string()))
}

/// Built-in filament presets, as a JSON array of `FilamentProfile`.
#[wasm_bindgen]
pub fn builtin_filaments() -> Result<String, JsError> {
    serde_json::to_string(&builtin::filaments()).map_err(|e| JsError::new(&e.to_string()))
}

/// Parses a sliced file's bytes into a `SliceSummary`, returned as JSON.
#[wasm_bindgen]
pub fn parse_slice_file(bytes: &[u8], filename: &str) -> Result<String, JsError> {
    let summary =
        formats::parse_slice_file(bytes, filename).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&summary).map_err(|e| JsError::new(&e.to_string()))
}

/// Costs a job given as TOML text, returning its `CostBreakdown` as JSON.
/// The job must be Bundled tier — embedded printer/filament profiles and
/// an embedded `SliceSummary` (`source.summary`, not `source.path`),
/// since a browser has no filesystem to resolve either a linked profile
/// name or a sliced-file path against.
#[wasm_bindgen]
pub fn calc_job(job_toml: &str) -> Result<String, JsError> {
    let job = ops::parse_job(job_toml).map_err(|e| JsError::new(&e.to_string()))?;
    let JobSource::Embedded(slice) = &job.source else {
        return Err(JsError::new(
            "job's [source] must be a Bundled `summary`, not a `path` — printcalc-wasm has no filesystem to re-parse a sliced file from",
        ));
    };
    let breakdown = ops::calc_job_data(&job, slice).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&breakdown).map_err(|e| JsError::new(&e.to_string()))
}
