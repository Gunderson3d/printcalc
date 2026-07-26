//! `Job`, `ProfileRef`, and the three job tiers (Default/Full/Bundled).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::filament::MaterialProfile;
use super::printer::PrinterProfile;
use crate::formats::slice_summary::SliceSummary;

/// Either a profile linked by name (resolved against the local library at
/// calc time) or a profile embedded directly in the job file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProfileRef<T> {
    /// Resolved by name against the local profile library at calc time.
    Linked {
        /// The profile's name, e.g. in the library under `printers/<name>.toml`.
        #[serde(rename = "ref")]
        name: String,
    },
    /// The profile's full data, stored directly in the job.
    Embedded {
        /// The embedded profile itself.
        profile: Box<T>,
    },
}

impl<T> ProfileRef<T> {
    /// The profile itself, if embedded. `Linked` refs must be resolved
    /// against the local profile library (see `disk.rs`) before calc.
    pub fn embedded(&self) -> Option<&T> {
        match self {
            ProfileRef::Embedded { profile } => Some(profile),
            ProfileRef::Linked { .. } => None,
        }
    }
}

/// Where a job's [`crate::formats::slice_summary::SliceSummary`] comes
/// from. `Path` is re-parsed at calc time (Default/Full tiers); `Embedded`
/// needs no sliced file at all (Bundled tier).
#[derive(Debug, Clone, PartialEq)]
pub enum JobSource {
    /// Re-parsed from this path (relative to the job file's own location,
    /// if not absolute) at calc time.
    Path(PathBuf),
    /// Parsed once and stored directly in the job.
    Embedded(SliceSummary),
}

/// Labor cost input for a job. No sensible global default for `minutes` —
/// unlike `rate_per_hour`, it's always job-specific.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LaborCost {
    /// `None` => fall back to the global config default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_per_hour: Option<f64>,
    /// Time actually spent on labor for this job.
    pub minutes: f64,
}

/// A flat per-piece extra cost (inserts, magnets, ...) added on top of
/// material/wear/electricity/labor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareItem {
    /// e.g. "brass insert M3x5".
    pub name: String,
    /// Cost per unit.
    pub cost: f64,
    /// How many were used.
    pub quantity: u32,
}

/// Slot id the `[filament]` (singular) sugar always maps to.
pub const SINGLE_FILAMENT_SLOT: &str = "1";

/// A costable print job: which sliced file (or embedded slice data), which
/// printer/filament profiles to price it against, and whatever job-
/// specific overrides/extras apply. Built via
/// [`crate::ops::new_job`]/[`crate::ops::new_job_from_slice`], costed via
/// [`super::cost::calculate_cost`].
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// A free-form label — not used for anything besides display/output.
    pub name: String,
    /// The sliced file this job costs (re-parsed at calc time), or its
    /// already-parsed data embedded directly.
    pub source: JobSource,
    /// `None` => every plate; `Some(i)` => just plate `i`.
    pub plate_selection: Option<u32>,
    /// Which printer profile to price wear/electricity against.
    pub printer: ProfileRef<PrinterProfile>,
    /// Keyed by `FilamentUsage::slot_id`. `[filament]` (singular) in TOML
    /// is sugar for a one-entry map under [`SINGLE_FILAMENT_SLOT`].
    pub filament: HashMap<String, ProfileRef<MaterialProfile>>,
    /// `None` => use each filament profile's own `efficiency_factor`.
    pub material_efficiency_factor_override: Option<f64>,
    /// `None` => fall back to the global config default.
    pub electricity_cost_per_kwh_override: Option<f64>,
    /// Labor time and (optionally) rate for this job.
    pub labor: LaborCost,
    /// Extra flat costs beyond material/wear/electricity/labor.
    pub hardware: Vec<HardwareItem>,
    /// `None` => fall back to the global config default (`[50.0, 60.0,
    /// 70.0]`).
    pub markup_percentages_override: Option<Vec<f64>>,
}

// Wire format mirrors the TOML shapes used on disk exactly. Hand-rolled
// (De)Serialize on Job instead of a derive, since "exactly one of
// `path`/`summary`" and "exactly one of `filament`/`filaments`" can't be
// expressed by derive alone.

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PlateWire {
    All(String),
    Index(u32),
}

#[derive(Serialize, Deserialize)]
struct SourceWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<SliceSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plate: Option<PlateWire>,
}

#[derive(Serialize, Deserialize)]
struct JobWire {
    name: String,
    source: SourceWire,
    printer: ProfileRef<PrinterProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filament: Option<ProfileRef<MaterialProfile>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filaments: Option<HashMap<String, ProfileRef<MaterialProfile>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    material_efficiency_factor_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    electricity_cost_per_kwh_override: Option<f64>,
    labor: LaborCost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hardware: Vec<HardwareItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    markup_percentages_override: Option<Vec<f64>>,
}

impl<'de> Deserialize<'de> for Job {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = JobWire::deserialize(deserializer)?;

        let source = match (wire.source.path, wire.source.summary) {
            (Some(path), None) => JobSource::Path(path),
            (None, Some(summary)) => JobSource::Embedded(summary),
            (None, None) => {
                return Err(D::Error::custom(
                    "[source] needs either `path` or `summary`",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(D::Error::custom(
                    "[source] can't have both `path` and `summary`",
                ));
            }
        };
        let plate_selection = match wire.source.plate {
            None => None,
            Some(PlateWire::Index(i)) => Some(i),
            Some(PlateWire::All(s)) if s == "all" => None,
            Some(PlateWire::All(s)) => {
                return Err(D::Error::custom(format!(
                    "invalid `[source] plate` value: {s:?} (expected \"all\" or a plate number)"
                )));
            }
        };

        let filament = match (wire.filament, wire.filaments) {
            (Some(single), None) => HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), single)]),
            (None, Some(map)) => map,
            (None, None) => {
                return Err(D::Error::custom(
                    "job needs either `[filament]` or `[filaments]`",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(D::Error::custom(
                    "job can't have both `[filament]` and `[filaments]`",
                ));
            }
        };

        Ok(Job {
            name: wire.name,
            source,
            plate_selection,
            printer: wire.printer,
            filament,
            material_efficiency_factor_override: wire.material_efficiency_factor_override,
            electricity_cost_per_kwh_override: wire.electricity_cost_per_kwh_override,
            labor: wire.labor,
            hardware: wire.hardware,
            markup_percentages_override: wire.markup_percentages_override,
        })
    }
}

impl Serialize for Job {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (path, summary) = match &self.source {
            JobSource::Path(path) => (Some(path.clone()), None),
            JobSource::Embedded(summary) => (None, Some(summary.clone())),
        };
        let source = SourceWire {
            path,
            summary,
            plate: self.plate_selection.map(PlateWire::Index),
        };

        // Only the single-slot-under-"1" shape collapses to `[filament]`;
        // everything else (including a one-entry map under any other slot
        // id) is spelled out as `[filaments]`.
        let (filament, filaments) =
            if self.filament.len() == 1 && self.filament.contains_key(SINGLE_FILAMENT_SLOT) {
                (self.filament.get(SINGLE_FILAMENT_SLOT).cloned(), None)
            } else {
                (None, Some(self.filament.clone()))
            };

        let wire = JobWire {
            name: self.name.clone(),
            source,
            printer: self.printer.clone(),
            filament,
            filaments,
            material_efficiency_factor_override: self.material_efficiency_factor_override,
            electricity_cost_per_kwh_override: self.electricity_cost_per_kwh_override,
            labor: self.labor,
            hardware: self.hardware.clone(),
            markup_percentages_override: self.markup_percentages_override.clone(),
        };
        wire.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::filament::MaterialKind;
    use crate::model::printer::PrinterKind;

    #[test]
    fn default_tier_single_filament_round_trips() {
        let toml_str = r#"
name = "widget-v2"

[source]
path = "prints/widget.gcode.3mf"

[printer]
ref = "bambu-a1"

[filament]
ref = "pla-black"

[labor]
minutes = 10.0

[[hardware]]
name = "brass insert M3x5"
cost = 0.12
quantity = 4
"#;
        let job: Job = toml::from_str(toml_str).unwrap();
        assert_eq!(job.name, "widget-v2");
        assert_eq!(
            job.source,
            JobSource::Path("prints/widget.gcode.3mf".into())
        );
        assert_eq!(job.plate_selection, None);
        assert_eq!(
            job.printer,
            ProfileRef::Linked {
                name: "bambu-a1".into()
            }
        );
        assert_eq!(job.filament.len(), 1);
        assert_eq!(
            job.filament.get(SINGLE_FILAMENT_SLOT),
            Some(&ProfileRef::Linked {
                name: "pla-black".into()
            })
        );
        assert_eq!(job.labor.minutes, 10.0);
        assert_eq!(job.hardware.len(), 1);

        let round_tripped: Job = toml::from_str(&toml::to_string(&job).unwrap()).unwrap();
        assert_eq!(round_tripped, job);
    }

    #[test]
    fn multi_filament_map_round_trips() {
        let toml_str = r#"
name = "multicolor-vase"

[source]
path = "prints/vase_ams.gcode.3mf"

[printer]
ref = "bambu-a1"

[filaments]
"1" = { ref = "pla-black" }
"2" = { ref = "pla-red" }
"4" = { ref = "petg-clear" }

[labor]
rate_per_hour = 15.0
minutes = 10.0
"#;
        let job: Job = toml::from_str(toml_str).unwrap();
        assert_eq!(job.filament.len(), 3);
        assert!(job.filament.contains_key("4"));

        let serialized = toml::to_string(&job).unwrap();
        assert!(serialized.contains("[filaments."));
        assert!(!serialized.contains("[filament]"));
        let round_tripped: Job = toml::from_str(&serialized).unwrap();
        assert_eq!(round_tripped, job);
    }

    #[test]
    fn bundled_tier_embeds_summary_and_profiles() {
        let toml_str = r#"
name = "widget-v2"

[source]

[source.summary]
source_format = "bambu-3mf"

[[source.summary.plates]]
plate_index = 1
print_time_seconds = 1037.0
[[source.summary.plates.filament_usage]]
slot_id = "1"
material_type = "PLA"
grams = 6.66

[printer.profile]
name = "bambu-a1"
kind = "fdm"
[printer.profile.wear]
purchase_price = 800.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 80.0
power_consumption_watts = 160.0
cost_buffer_factor = 1.3

[filament.profile]
name = "pla-black"
kind = "filament"
price_per_kg = 18.99
efficiency_factor = 1.1

[labor]
rate_per_hour = 15.0
minutes = 10.0
"#;
        let job: Job = toml::from_str(toml_str).unwrap();
        let JobSource::Embedded(summary) = &job.source else {
            panic!("expected an embedded summary");
        };
        assert_eq!(summary.plates.len(), 1);
        let printer = job.printer.embedded().unwrap();
        assert_eq!(printer.name, "bambu-a1");
        assert_eq!(printer.kind, PrinterKind::Fdm);
        let filament = job.filament[SINGLE_FILAMENT_SLOT].embedded().unwrap();
        assert_eq!(filament.kind, MaterialKind::Filament);

        let round_tripped: Job = toml::from_str(&toml::to_string(&job).unwrap()).unwrap();
        assert_eq!(round_tripped, job);
    }

    #[test]
    fn missing_filament_and_filaments_is_an_error() {
        let toml_str = r#"
name = "broken"

[source]
path = "prints/widget.gcode.3mf"

[printer]
ref = "bambu-a1"

[labor]
minutes = 10.0
"#;
        assert!(toml::from_str::<Job>(toml_str).is_err());
    }
}
