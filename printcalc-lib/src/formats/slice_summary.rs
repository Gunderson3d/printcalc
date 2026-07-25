//! `SliceSummary`: the common shape extracted from any supported slicer
//! output. See `SPEC.md` §2.
//!
//! Always compiled regardless of which parser features are enabled — a
//! Bundled job (§5) embeds a `SliceSummary` directly and needs no parser at
//! calc time at all.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Which slicer/format a [`SliceSummary`] was extracted from. `SPEC.md` §4.
///
/// `#[non_exhaustive]`: more formats land over time (`SPEC.md` §9), and
/// each is a new variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SourceFormat {
    /// Bambu Studio `.gcode.3mf`.
    #[serde(rename = "bambu-3mf")]
    Bambu3mf,
    /// PrusaSlicer/OrcaSlicer plain-text `.gcode`.
    #[serde(rename = "prusa-gcode")]
    PrusaGcode,
    /// PrusaSlicer binary `.bgcode`.
    #[serde(rename = "prusa-bgcode")]
    PrusaBgcode,
    /// PrusaSlicer resin `.sl1`/`.sl1s`.
    #[serde(rename = "prusa-sl1")]
    PrusaSl1,
}

/// The common shape every format parser ([`crate::formats::SliceParser`])
/// extracts from a sliced file — print time and material usage, per plate.
/// See `SPEC.md` §2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceSummary {
    /// Which parser produced this summary.
    pub source_format: SourceFormat,
    /// Always at least one entry — single-plate formats (Prusa gcode/
    /// bgcode/sl1/sl1s) produce exactly one.
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

/// One plate's worth of a sliced file — print time and every filament slot
/// with nonzero usage on it. `SPEC.md` §2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateSummary {
    /// 1-based, matches the slicer's own plate numbering.
    pub plate_index: u32,
    /// How long this plate took (or was predicted) to print.
    pub print_time_seconds: f64,
    /// One entry per slot/tray/extruder that deposited material on this
    /// plate — a plain single-material print still has exactly one.
    pub filament_usage: Vec<FilamentUsage>,
    /// The printer model the slicer targeted, if the format reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printer_model: Option<String>,
    /// Layer height in mm, if the format reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_height: Option<f64>,
    /// Anything format-specific worth keeping that doesn't have a home in
    /// the fields above.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

/// One filament/resin slot's usage on a plate — a Bambu AMS tray, a
/// multi-extruder Prusa slot, or the sole slot of a single-material print.
/// `SPEC.md` §2, §4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentUsage {
    /// Opaque, format-native slot id — whatever the slicer itself uses to
    /// distinguish slots (AMS tray `id`, Prusa extruder index, ...).
    pub slot_id: String,
    /// Free-form material name/description as the slicer reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_type: Option<String>,
    /// Free-form display color, e.g. a hex code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Set for filament (weight-measured) usage; `None` for resin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grams: Option<f64>,
    /// Set for resin (volume-measured) usage; `None` for filament.
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
