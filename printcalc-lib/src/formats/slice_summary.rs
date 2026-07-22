//! `SliceSummary`: the common shape extracted from any supported slicer
//! output. See `SPEC.md` §2.
//!
//! Always compiled regardless of which parser features are enabled — a
//! Bundled job (§5) embeds a `SliceSummary` directly and needs no parser at
//! calc time at all.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceFormat {
    #[serde(rename = "bambu-3mf")]
    Bambu3mf,
    #[serde(rename = "prusa-gcode")]
    PrusaGcode,
    #[serde(rename = "prusa-bgcode")]
    PrusaBgcode,
    #[serde(rename = "prusa-sl1")]
    PrusaSl1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceSummary {
    pub source_format: SourceFormat,
    pub plates: Vec<PlateSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateSummary {
    pub plate_index: u32,
    pub print_time_seconds: f64,
    pub filament_usage: Vec<FilamentUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printer_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_height: Option<f64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentUsage {
    pub slot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grams: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ml: Option<f64>,
}
