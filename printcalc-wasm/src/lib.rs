//! WASM bindings over `libprintcalc` for in-browser use, mirroring the
//! lib's crate-root API one-for-one for everything that makes sense
//! without a filesystem: `parse_slice_bytes`, `parse_job`, `new_job`, and
//! `calculate`, plus the `calc_job` one-shot (TOML in, breakdown out)
//! and the built-in preset listings. The lib's `disk` helpers
//! (`read_*`, `Job::slice_summary` on a `Path` source) have no counterpart
//! here — a browser has no filesystem.
//!
//! Thin `wasm-bindgen` wrapper only — no business logic of its own. These
//! functions return a JSON `String` rather than a `JsValue`
//! (`JSON.parse()` it on the JS side) — same "SliceSummary/Job/
//! CostBreakdown as JSON" contract, without needing `serde-wasm-bindgen`
//! (not otherwise a dependency anywhere in this workspace) just to hand
//! back a native JS object instead of a string.
//!
//! Profile refs in a job costed here must be embedded profiles or names of
//! built-in presets (the same rule as the lib's `calculate`), and its
//! source must be embedded too — there's no library or sliced file on disk
//! to resolve a `Linked` name or a `path` against.

use wasm_bindgen::prelude::*;

use libprintcalc::{
    CostDefaults, HardwareItem, Job, JobSource, LaborCost, MaterialProfile, PrinterProfile,
    ProfileRef, SliceSummary, auto_job as lib_auto_job, builtin, calculate as lib_calculate,
    new_job as lib_new_job, parse_job as lib_parse_job, parse_material as lib_parse_material,
    parse_printer as lib_parse_printer, parse_slice_bytes as lib_parse_slice_bytes,
};
use std::collections::HashMap;

#[wasm_bindgen]
pub fn printcalc_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Built-in printer presets (see `libprintcalc::builtin`), as a JSON
/// array of `PrinterProfile` — lets the host page offer them without a
/// filesystem to load a saved library from.
#[wasm_bindgen]
pub fn builtin_printers() -> Result<String, JsError> {
    serde_json::to_string(&builtin::printers()).map_err(|e| JsError::new(&e.to_string()))
}

/// Built-in filament presets, as a JSON array of `MaterialProfile`.
#[wasm_bindgen]
pub fn builtin_filaments() -> Result<String, JsError> {
    serde_json::to_string(&builtin::filaments()).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::parse_slice_bytes`: parses a sliced file's
/// bytes into a `SliceSummary`, returned as JSON. `filename` is a
/// detection hint only.
#[wasm_bindgen]
pub fn parse_slice_bytes(bytes: &[u8], filename: &str) -> Result<String, JsError> {
    let summary =
        lib_parse_slice_bytes(bytes, filename).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&summary).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::parse_job`: parses job TOML into a `Job`,
/// returned as JSON.
#[wasm_bindgen]
pub fn parse_job(job_toml: &str) -> Result<String, JsError> {
    let job = lib_parse_job(job_toml).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&job).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::parse_printer`: parses printer profile TOML
/// into a `PrinterProfile`, returned as JSON. Drop it into a job JSON's
/// `printer` as `{ "profile": <this JSON> }` to cost against it via
/// [`calculate`].
#[wasm_bindgen]
pub fn parse_printer(printer_toml: &str) -> Result<String, JsError> {
    let printer = lib_parse_printer(printer_toml).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&printer).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::parse_material`: parses filament/resin profile
/// TOML into a `MaterialProfile`, returned as JSON. Drop it into a job
/// JSON's `material` as `"1": { "profile": <this JSON> }` (per slot) to
/// cost against it via [`calculate`].
#[wasm_bindgen]
pub fn parse_material(material_toml: &str) -> Result<String, JsError> {
    let material = lib_parse_material(material_toml).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&material).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::new_job`: builds a `Job` with the printer
/// embedded by value, returned as JSON. `source_json` is either a JSON
/// string (a sliced-file path, re-parsed at calc time — not here, a
/// browser has no filesystem) or a `SliceSummary` JSON object (embedded
/// directly). `printer_json` is a `PrinterProfile` JSON object (e.g. from
/// [`parse_printer`] or [`builtin_printers`]); `materials_json` maps slot
/// id to a profile ref (`{"1": {"ref": "pla-black"}}` or
/// `{"1": {"profile": …}}`); `labor_json` is an optional
/// `{"rate_per_hour": …, "minutes": …}` object; `hardware_json` an
/// optional `[{"name": …, "cost": …, "quantity": …}]` array.
#[wasm_bindgen]
pub fn new_job(
    name: &str,
    source_json: &str,
    printer_json: &str,
    materials_json: &str,
    labor_json: Option<String>,
    hardware_json: Option<String>,
) -> Result<String, JsError> {
    let source: JobSource = match serde_json::from_str(source_json)
        .map_err(|e| JsError::new(&format!("source JSON: {e}")))?
    {
        serde_json::Value::String(path) => JobSource::from(path),
        other => JobSource::from(
            serde_json::from_value::<SliceSummary>(other)
                .map_err(|e| JsError::new(&format!("source summary JSON: {e}")))?,
        ),
    };
    let printer: PrinterProfile = serde_json::from_str(printer_json)
        .map_err(|e| JsError::new(&format!("printer JSON: {e}")))?;
    let materials: HashMap<String, ProfileRef<MaterialProfile>> =
        serde_json::from_str(materials_json)
            .map_err(|e| JsError::new(&format!("materials JSON: {e}")))?;
    let labor: Option<LaborCost> = labor_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| JsError::new(&format!("labor JSON: {e}")))?;
    let hardware: Vec<HardwareItem> = hardware_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| JsError::new(&format!("hardware JSON: {e}")))?
        .unwrap_or_default();
    let job = lib_new_job(name, source, printer, materials, labor, hardware);
    serde_json::to_string(&job).map_err(|e| JsError::new(&e.to_string()))
}

/// Mirrors `libprintcalc::auto_job`: builds a job from a sliced file
/// alone, taking the printer and each filament slot from what the file
/// itself records.
///
/// `slice_json` is a `SliceSummary` (from [`parse_slice_bytes`]).
/// `candidate_printers_json`/`candidate_materials_json` are optional JSON
/// arrays of profiles to search ahead of the built-in presets — pass the
/// page's own saved profiles here, or `null` for built-ins only.
/// `default_efficiency_factor` is the waste allowance given to a profile
/// built from the file's own stated price (no slicer records one); `1.1`
/// matches the built-in presets.
///
/// Returns `{ "job": Job, "printer": Match|null, "slots": [...] }` — the
/// job is ready for [`calculate`], and `printer`/`slots` describe what was
/// guessed and how confidently, so the page can show it and let the user
/// correct it. A slot that matched nothing and had no price in the file is
/// left out of the job, so costing it fails loudly rather than pricing it
/// at zero.
#[wasm_bindgen]
pub fn auto_job(
    name: &str,
    slice_json: &str,
    candidate_printers_json: Option<String>,
    candidate_materials_json: Option<String>,
    default_efficiency_factor: f64,
) -> Result<String, JsError> {
    let slice: SliceSummary =
        serde_json::from_str(slice_json).map_err(|e| JsError::new(&format!("slice JSON: {e}")))?;
    let printers: Vec<PrinterProfile> = parse_optional_array(candidate_printers_json, "printers")?;
    let materials: Vec<MaterialProfile> =
        parse_optional_array(candidate_materials_json, "materials")?;

    let auto = lib_auto_job(
        name,
        slice.clone(),
        slice,
        printers.iter(),
        materials.iter(),
        default_efficiency_factor,
    );

    // `AutoJob`/`AutoSlot`/`Match` aren't `Serialize` (they carry no wire
    // format of their own), so the shape is assembled here.
    let slots: Vec<serde_json::Value> = auto
        .slots
        .iter()
        .map(|slot| {
            serde_json::json!({
                "usage": slot.usage,
                "matched": slot.matched.as_ref().map(|m| serde_json::json!({
                    "profile": m.profile,
                    "confidence": m.confidence,
                    "matched_on": m.matched_on,
                })),
                "from_file": slot.from_file,
            })
        })
        .collect();
    let out = serde_json::json!({
        "job": auto.job,
        "printer": auto.printer.as_ref().map(|m| serde_json::json!({
            "profile": m.profile,
            "confidence": m.confidence,
            "matched_on": m.matched_on,
        })),
        "slots": slots,
    });
    serde_json::to_string(&out).map_err(|e| JsError::new(&e.to_string()))
}

/// `null`/absent is an empty list; anything else must be a JSON array of
/// profiles.
fn parse_optional_array<T: serde::de::DeserializeOwned>(
    json: Option<String>,
    what: &str,
) -> Result<Vec<T>, JsError> {
    match json {
        None => Ok(Vec::new()),
        Some(text) if text.trim().is_empty() || text.trim() == "null" => Ok(Vec::new()),
        Some(text) => serde_json::from_str(&text)
            .map_err(|e| JsError::new(&format!("candidate {what} JSON: {e}"))),
    }
}

/// Mirrors `libprintcalc::calculate`: costs a `job` (JSON) end to end,
/// returning the `CostBreakdown` as JSON. The job's source must be an
/// embedded `SliceSummary` — a browser has no filesystem to re-parse a
/// `path` source from. `defaults_json` may be `null`/`undefined` to use
/// `CostDefaults::default()`.
#[wasm_bindgen]
pub fn calculate(job_json: &str, defaults_json: Option<String>) -> Result<String, JsError> {
    let job: Job =
        serde_json::from_str(job_json).map_err(|e| JsError::new(&format!("job JSON: {e}")))?;
    if !matches!(job.source, JobSource::Embedded(_)) {
        return Err(JsError::new(
            "job's [source] must be a Bundled `summary`, not a `path` — printcalc-wasm has no filesystem to re-parse a sliced file from",
        ));
    }
    let defaults = match defaults_json {
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| JsError::new(&format!("defaults JSON: {e}")))?
        }
        None => CostDefaults::default(),
    };
    let breakdown = lib_calculate(&job, &defaults).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&breakdown).map_err(|e| JsError::new(&e.to_string()))
}

/// Costs a job given as TOML text, returning its `CostBreakdown` as JSON.
/// The job must be Bundled tier — embedded printer/filament profiles and
/// an embedded `SliceSummary` (`source.summary`, not `source.path`),
/// since a browser has no filesystem to resolve either a linked profile
/// name or a sliced-file path against.
#[wasm_bindgen]
pub fn calc_job(job_toml: &str) -> Result<String, JsError> {
    let job = lib_parse_job(job_toml).map_err(|e| JsError::new(&e.to_string()))?;
    if !matches!(job.source, JobSource::Embedded(_)) {
        return Err(JsError::new(
            "job's [source] must be a Bundled `summary`, not a `path` — printcalc-wasm has no filesystem to re-parse a sliced file from",
        ));
    }
    let breakdown =
        lib_calculate(&job, &CostDefaults::default()).map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&breakdown).map_err(|e| JsError::new(&e.to_string()))
}
