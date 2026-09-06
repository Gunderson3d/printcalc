//! `SliceSummary`: the common shape extracted from any supported slicer
//! output.
//!
//! Two halves, with very different obligations.
//!
//! The **usage** half — plates, print times, and per-slot grams or
//! millilitres — is what costing consumes, and every parser must produce
//! it.
//!
//! The **metadata** half — [`SliceMeta`] and the profile names, vendors,
//! prices and colours on each [`FilamentUsage`] — is entirely optional
//! and feeds [`crate::matching`]. A format that records less still costs
//! correctly; it just asks the user more questions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Which slicer/format a [`SliceSummary`] was extracted from.
///
/// `#[non_exhaustive]`: more formats land over time, and each is a new
/// variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SourceFormat {
    /// Bambu Studio `.gcode.3mf`.
    #[serde(rename = "bambu-3mf")]
    Bambu3mf,
    /// Plain-text `.gcode` with PrusaSlicer-style metadata comments (most
    /// FDM slicers). Deserialize alias keeps job files written with the old
    /// `prusa-gcode` name loading.
    #[serde(rename = "gcode", alias = "prusa-gcode")]
    Gcode,
    /// PrusaSlicer binary `.bgcode`.
    #[serde(rename = "prusa-bgcode")]
    PrusaBgcode,
    /// PrusaSlicer resin `.sl1`/`.sl1s`.
    #[serde(rename = "prusa-sl1")]
    PrusaSl1,
}

/// File-wide identity a slicer recorded about the setup that produced a
/// sliced file: which printer profile, which print profile, which nozzle,
/// which slicer build. None of it is needed to *cost* a job — it's what
/// [`crate::matching`] uses to guess printer/material profiles so the
/// caller doesn't have to name them by hand.
///
/// Everything is optional and best-effort: a format that doesn't report a
/// field just leaves it `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SliceMeta {
    /// The slicer's own printer *profile* name, e.g. `"Prusa MINI 0.4
    /// nozzle"` or `"Bambu Lab P2S 0.4 nozzle"` — more specific than
    /// [`PlateSummary::printer_model`], which is just the machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer_profile: Option<String>,
    /// The slicer's print/quality profile, e.g. `"0.20mm Standard @MINI"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_profile: Option<String>,
    /// Printer variant as the slicer labels it, usually the nozzle
    /// (`"0.4"`, `"HF0.4"`, `"default"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer_variant: Option<String>,
    /// Nozzle diameter in mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nozzle_diameter: Option<f64>,
    /// Which slicer wrote the file, with its version if reported, e.g.
    /// `"PrusaSlicer-2.9.6+flathub.org"` or `"BambuStudio 02.07.01.62"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slicer: Option<String>,
}

impl SliceMeta {
    /// Whether every field is unset — the "format reported nothing" case,
    /// skipped entirely when serializing.
    pub fn is_empty(&self) -> bool {
        *self == SliceMeta::default()
    }
}

/// The common shape every format parser ([`crate::formats::SliceParser`])
/// extracts from a sliced file — print time and material usage, per plate,
/// plus whatever identifying [`SliceMeta`] the format carried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceSummary {
    /// Which parser produced this summary.
    pub source_format: SourceFormat,
    /// File-wide slicer/printer identity, for profile auto-matching. Older
    /// job files predate this field, so it defaults to empty.
    #[serde(default, skip_serializing_if = "SliceMeta::is_empty")]
    pub meta: SliceMeta,
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
    /// slots from `0` even when only one ends up used.
    pub fn distinct_filament_slots(&self) -> Vec<String> {
        self.slots().into_iter().map(|s| s.slot_id).collect()
    }

    /// One [`SlotUsage`] per slot with nonzero usage anywhere in the file,
    /// sorted by slot id — each slot's total usage plus whatever the
    /// slicer said about the filament loaded in it.
    ///
    /// This is the "everything the file knows about slot N" view:
    /// [`crate::matching`] guesses a material profile from it, the CLI
    /// prints it, and [`crate::model::cost::CalcError::MissingMaterial`]
    /// names the unmapped ones with their type and color rather than a
    /// bare slot number.
    pub fn slots(&self) -> Vec<SlotUsage> {
        let mut slots: Vec<SlotUsage> = Vec::new();
        for usage in self.plates.iter().flat_map(|p| &p.filament_usage) {
            if usage.grams.unwrap_or(0.0) == 0.0 && usage.ml.unwrap_or(0.0) == 0.0 {
                continue; // tray/extruder present but not used on this print
            }
            match slots.iter_mut().find(|s| s.slot_id == usage.slot_id) {
                Some(slot) => slot.add(usage),
                None => slots.push(SlotUsage::from_usage(usage)),
            }
        }
        slots.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));
        slots
    }

    /// The printer the slicer targeted, as reported by the first plate
    /// that names one — every format that reports it at all reports the
    /// same value for every plate.
    pub fn printer_model(&self) -> Option<&str> {
        self.plates.iter().find_map(|p| p.printer_model.as_deref())
    }

    /// The first plate's layer height, same rule as [`Self::printer_model`].
    pub fn layer_height(&self) -> Option<f64> {
        self.plates.iter().find_map(|p| p.layer_height)
    }

    /// Total print time across every plate, in seconds.
    pub fn total_print_time_seconds(&self) -> f64 {
        self.plates.iter().map(|p| p.print_time_seconds).sum()
    }
}

/// One plate's worth of a sliced file — print time and every filament slot
/// with nonzero usage on it.
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
///
/// Beyond the amount used, every field here is the slicer's own
/// description of what was loaded in the slot. It's all optional: a format
/// that doesn't report a field leaves it `None`, and nothing in the cost
/// calc depends on any of it — it exists so a job can be built without the
/// caller retyping what the file already says.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FilamentUsage {
    /// Opaque, format-native slot id — whatever the slicer itself uses to
    /// distinguish slots (AMS tray `id`, Prusa extruder index, ...).
    pub slot_id: String,
    /// Free-form material name/description as the slicer reports it,
    /// usually the family (`"PLA"`, `"PETG"`); for resin, the full
    /// material name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_type: Option<String>,
    /// Free-form display color, e.g. a hex code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The slicer's own filament/resin *profile* name, e.g. `"Generic PLA
    /// @System"`, `"Bambu PLA Basic @BBL P2S"`, or an SLA material name.
    /// The strongest hint there is for matching a stored profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    /// Manufacturer as the slicer reports it, e.g. `"Bambu Lab"`,
    /// `"Prusa Polymers"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// The price the slicer's own filament profile carries — per kg for
    /// filament, per liter for resin, matching
    /// [`crate::MaterialProfile::price`]. Lets a job be costed off the
    /// file alone when no stored profile matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// Material density in g/cm³, if reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,
    /// Length of filament used, in meters, if reported. Informational —
    /// costing goes by `grams`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meters: Option<f64>,
    /// Set for filament (weight-measured) usage; `None` for resin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grams: Option<f64>,
    /// Set for resin (volume-measured) usage; `None` for filament.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ml: Option<f64>,
}

/// One slot's totals across every plate, plus the slicer's description of
/// the filament in it. Built by [`SliceSummary::slots`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SlotUsage {
    /// The slot's format-native id, as in [`FilamentUsage::slot_id`].
    pub slot_id: String,
    /// First non-empty [`FilamentUsage::material_type`] seen for the slot.
    pub material_type: Option<String>,
    /// First non-empty [`FilamentUsage::color`] seen for the slot.
    pub color: Option<String>,
    /// First non-empty [`FilamentUsage::profile_name`] seen for the slot.
    pub profile_name: Option<String>,
    /// First non-empty [`FilamentUsage::vendor`] seen for the slot.
    pub vendor: Option<String>,
    /// First [`FilamentUsage::price`] seen for the slot.
    pub price: Option<f64>,
    /// Total grams across every plate (`0.0` for resin).
    pub grams: f64,
    /// Total ml across every plate (`0.0` for filament).
    pub ml: f64,
}

impl SlotUsage {
    fn from_usage(usage: &FilamentUsage) -> Self {
        SlotUsage {
            slot_id: usage.slot_id.clone(),
            material_type: nonempty(usage.material_type.as_deref()),
            color: nonempty(usage.color.as_deref()),
            profile_name: nonempty(usage.profile_name.as_deref()),
            vendor: nonempty(usage.vendor.as_deref()),
            price: usage.price,
            grams: usage.grams.unwrap_or(0.0),
            ml: usage.ml.unwrap_or(0.0),
        }
    }

    fn add(&mut self, usage: &FilamentUsage) {
        self.grams += usage.grams.unwrap_or(0.0);
        self.ml += usage.ml.unwrap_or(0.0);
        // First plate to describe the slot wins; later plates only fill
        // in what it left blank.
        fill(&mut self.material_type, usage.material_type.as_deref());
        fill(&mut self.color, usage.color.as_deref());
        fill(&mut self.profile_name, usage.profile_name.as_deref());
        fill(&mut self.vendor, usage.vendor.as_deref());
        if self.price.is_none() {
            self.price = usage.price;
        }
    }

    /// A short human label for the slot, as used in error messages and
    /// CLI output: `"slot 1 (PLA #AE6342)"`, degrading to just
    /// `"slot 1"` when the file said nothing about it.
    pub fn label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(name) = self.profile_name.as_deref().filter(|s| !s.is_empty()) {
            parts.push(name);
        } else if let Some(kind) = self.material_type.as_deref().filter(|s| !s.is_empty()) {
            parts.push(kind);
        }
        if let Some(color) = self.color.as_deref().filter(|s| !s.is_empty()) {
            parts.push(color);
        }
        if parts.is_empty() {
            format!("slot {}", self.slot_id)
        } else {
            format!("slot {} ({})", self.slot_id, parts.join(" "))
        }
    }
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value.filter(|s| !s.is_empty()).map(String::from)
}

/// Sets `target` from `value` only if it's still unset and `value` is
/// something — so a later plate that says nothing can't overwrite an
/// earlier plate's description with a blank.
fn fill(target: &mut Option<String>, value: Option<&str>) {
    if target.is_none() {
        *target = nonempty(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(slot_id: &str, grams: f64) -> FilamentUsage {
        FilamentUsage {
            slot_id: slot_id.into(),
            grams: Some(grams),
            ..FilamentUsage::default()
        }
    }

    fn plate(index: u32, usage: Vec<FilamentUsage>) -> PlateSummary {
        PlateSummary {
            plate_index: index,
            print_time_seconds: 100.0,
            filament_usage: usage,
            printer_model: None,
            layer_height: None,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn distinct_filament_slots_dedupes_sorts_and_skips_zero_usage() {
        let summary = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            meta: SliceMeta::default(),
            plates: vec![
                plate(1, vec![usage("2", 1.0), usage("1", 2.0), usage("3", 0.0)]),
                plate(2, vec![usage("1", 1.0)]),
            ],
        };
        assert_eq!(
            summary.distinct_filament_slots(),
            vec!["1".to_string(), "2".to_string()]
        );
    }

    /// `slots()` totals a slot's usage across plates and keeps the first
    /// description the file gave it.
    #[test]
    fn slots_aggregate_usage_and_carry_slicer_metadata() {
        let described = FilamentUsage {
            slot_id: "1".into(),
            material_type: Some("PLA".into()),
            color: Some("#AE6342".into()),
            profile_name: Some("Bambu PLA Basic @BBL P2S".into()),
            vendor: Some("Bambu Lab".into()),
            price: Some(24.99),
            grams: Some(4.0),
            ..FilamentUsage::default()
        };
        let summary = SliceSummary {
            source_format: SourceFormat::Bambu3mf,
            meta: SliceMeta::default(),
            plates: vec![plate(1, vec![described]), plate(2, vec![usage("1", 2.5)])],
        };

        let slots = summary.slots();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].grams, 6.5);
        assert_eq!(slots[0].price, Some(24.99));
        assert_eq!(
            slots[0].profile_name.as_deref(),
            Some("Bambu PLA Basic @BBL P2S")
        );
        assert_eq!(
            slots[0].label(),
            "slot 1 (Bambu PLA Basic @BBL P2S #AE6342)"
        );
    }

    /// A slot the file said nothing about still gets a usable label.
    #[test]
    fn bare_slot_label_is_just_the_slot_id() {
        let summary = SliceSummary {
            source_format: SourceFormat::Gcode,
            meta: SliceMeta::default(),
            plates: vec![plate(1, vec![usage("0", 1.0)])],
        };
        assert_eq!(summary.slots()[0].label(), "slot 0");
    }

    /// An all-`None` `SliceMeta` serializes away entirely, so summaries
    /// from formats that report nothing look exactly as they did before.
    #[test]
    fn empty_meta_is_skipped_when_serializing() {
        let summary = SliceSummary {
            source_format: SourceFormat::Gcode,
            meta: SliceMeta::default(),
            plates: vec![plate(1, vec![usage("1", 1.0)])],
        };
        let text = toml::to_string(&summary).unwrap();
        assert!(!text.contains("meta"));
        assert_eq!(toml::from_str::<SliceSummary>(&text).unwrap(), summary);
    }
}
