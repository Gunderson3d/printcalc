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

impl SliceSummary {
    /// Every distinct `FilamentUsage::slot_id` with nonzero usage on any
    /// plate, sorted. Used to decide whether a job needs one filament
    /// (`--filament`) or a per-slot map (`--filament-map`) — and, for the
    /// single-slot case, which slot_id that one filament actually belongs
    /// to. That's not always `"1"`: Bambu single-tray and Prusa's
    /// scalar single-extruder case both normalize to `"1"`, but a
    /// multi-extruder-*capable* printer's comma-list metadata numbers
    /// slots from `0` even when only one ends up used (`SPEC.md` §4).
    pub fn distinct_filament_slots(&self) -> Vec<String> {
        let mut slots: Vec<String> = self
            .plates
            .iter()
            .flat_map(|p| &p.filament_usage)
            .filter(|u| u.grams.unwrap_or(0.0) != 0.0 || u.ml.unwrap_or(0.0) != 0.0)
            .map(|u| u.slot_id.clone())
            .collect();
        slots.sort();
        slots.dedup();
        slots
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(slot_id: &str, grams: f64) -> FilamentUsage {
        FilamentUsage { slot_id: slot_id.into(), material_type: None, color: None, grams: Some(grams), ml: None }
    }

    #[test]
    fn distinct_filament_slots_dedupes_sorts_and_skips_zero_usage() {
        let summary = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            plates: vec![
                PlateSummary {
                    plate_index: 1,
                    print_time_seconds: 100.0,
                    filament_usage: vec![usage("2", 1.0), usage("1", 2.0), usage("3", 0.0)],
                    printer_model: None,
                    layer_height: None,
                    extra: HashMap::new(),
                },
                PlateSummary {
                    plate_index: 2,
                    print_time_seconds: 100.0,
                    filament_usage: vec![usage("1", 1.0)], // repeated across plates
                    printer_model: None,
                    layer_height: None,
                    extra: HashMap::new(),
                },
            ],
        };
        assert_eq!(summary.distinct_filament_slots(), vec!["1".to_string(), "2".to_string()]);
    }
}
