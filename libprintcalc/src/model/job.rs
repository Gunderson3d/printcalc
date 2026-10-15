//! `Job`, `ProfileRef`, and the three job tiers (Default/Full/Bundled).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::filament::MaterialProfile;
use super::printer::PrinterProfile;
use crate::formats::slice_summary::SliceSummary;
use crate::{Error, parse_material, parse_printer, resolve_materials, resolve_printer};

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

impl From<String> for JobSource {
    fn from(path: String) -> Self {
        JobSource::Path(PathBuf::from(path))
    }
}

impl From<PathBuf> for JobSource {
    fn from(path: PathBuf) -> Self {
        JobSource::Path(path)
    }
}

impl From<SliceSummary> for JobSource {
    fn from(summary: SliceSummary) -> Self {
        JobSource::Embedded(summary)
    }
}

/// Labor cost input for a job. No sensible global default for `minutes` —
/// unlike `rate_per_hour`, it's always job-specific.
///
/// `#[non_exhaustive]`: build one with [`LaborCost::new`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LaborCost {
    /// `None` => fall back to the global config default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_per_hour: Option<f64>,
    /// Hands-on time per unit — setup, removal, support removal,
    /// finishing. Multiplied by `Job::quantity` for the batch, so charging
    /// a one-off setup across a batch means dividing it down yourself.
    pub minutes: f64,
}

impl LaborCost {
    /// `minutes` of hands-on time per unit, at the global default rate.
    pub fn new(minutes: f64) -> Self {
        LaborCost {
            rate_per_hour: None,
            minutes,
        }
    }

    /// Sets a job-specific hourly rate instead of the global default.
    pub fn with_rate_per_hour(mut self, rate: f64) -> Self {
        self.rate_per_hour = Some(rate);
        self
    }
}

/// A flat per-piece extra cost (inserts, magnets, ...) added on top of
/// material/wear/electricity/labor.
///
/// `#[non_exhaustive]`: build one with [`HardwareItem::new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HardwareItem {
    /// e.g. "brass insert M3x5".
    pub name: String,
    /// Cost per unit.
    pub cost: f64,
    /// How many were used **per unit** — 4 magnets per print stays 4
    /// however large the batch, and `Job::quantity` scales it.
    /// Defaults to 1.
    #[serde(default = "one")]
    pub quantity: u32,
}

impl HardwareItem {
    /// One of `name` per unit at `cost` each.
    pub fn new(name: impl Into<String>, cost: f64) -> Self {
        HardwareItem {
            name: name.into(),
            cost,
            quantity: 1,
        }
    }

    /// Sets how many are used per unit.
    pub fn with_quantity(mut self, quantity: u32) -> Self {
        self.quantity = quantity;
        self
    }
}

/// Slot id the `[material]` (singular) sugar always maps to.
pub const SINGLE_MATERIAL_SLOT: &str = "1";

/// A costable print job: which sliced file (or embedded slice data), which
/// printer/material profiles to price it against, and whatever job-specific overrides/extras apply.
/// Costed via [`super::cost::calculate_cost`] once its refs are resolved.
///
/// `#[non_exhaustive]`: build one with [`Job::new`] and the `with_*`
/// setters (or [`crate::new_job`]/[`crate::auto_job`]), or parse one
/// from TOML. Fields stay public to read and assign.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Job {
    /// A free-form label — not used for anything besides display/output.
    pub name: String,
    /// The sliced file this job costs (reparsed at calc time), or its
    /// already-parsed data embedded directly.
    pub source: JobSource,
    /// `None` => every plate; `Some(i)` => just plate `i`.
    pub plate_selection: Option<u32>,
    /// Which printer profile to price wear/electricity against.
    pub printer: ProfileRef<PrinterProfile>,
    /// Keyed by `FilamentUsage::slot_id`. `[material]` (singular) in TOML
    /// is sugar for a one-entry map under [`SINGLE_MATERIAL_SLOT`].
    pub material: HashMap<String, ProfileRef<MaterialProfile>>,
    /// Priced for any used slot `material` doesn't cover. `None` means
    /// every used slot must be mapped explicitly — the calc errors rather
    /// than pricing an unmapped slot at zero.
    pub default_material: Option<ProfileRef<MaterialProfile>>,
    /// How many of this job are being made. All five cost components, and
    /// so the total and the suggested prices, are for the whole batch.
    /// `1` for a one-off.
    pub quantity: u32,
    /// `None` => use each material profile's own `efficiency_factor`.
    pub material_efficiency_factor_override: Option<f64>,
    /// `None` => fall back to the global config default.
    pub electricity_cost_per_kwh_override: Option<f64>,
    /// Labor time and (optionally) rate for this job.
    pub labor: LaborCost,
    /// Extra flat costs beyond material/wear/electricity/labor.
    pub hardware: Vec<HardwareItem>,
    /// Gross margin percentages to suggest sale prices at. `None` => fall
    /// back to the global config default (`[50.0, 60.0, 70.0]`). See
    /// [`super::cost::SuggestedPrice`] — these are margins, not markups.
    pub margin_percentages_override: Option<Vec<f64>>,
}

/// How much of a job to embed when serializing it with
/// [`Job::to_toml`]. Serialization only ever goes *up* the tiers
/// (Linked → Embedded, Path → Embedded summary) — there's no way back
/// down, since a `Linked` ref's name and a `Path` source's original
/// location aren't recoverable once resolved away.
///
/// `#[non_exhaustive]`: match with a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum JobTier {
    /// Serialize as-is: `Linked` refs stay linked, `Embedded` stay
    /// embedded, the source is untouched.
    #[default]
    Slim,
    /// Printer and every material serialize as [`ProfileRef::Embedded`]
    /// — `Linked` refs are resolved against the [`crate::builtin`]
    /// presets. The source is untouched.
    Full,
    /// Like [`JobTier::Full`], plus the source becomes
    /// [`JobSource::Embedded`] — a `Path` source is read and parsed (see
    /// [`Job::slice_summary`]), so the result is fully self-contained.
    Bundled,
}

// Wire format mirrors the TOML shapes used on disk exactly. Hand-rolled
// (De)Serialize on Job instead of a derived, since "exactly one of
// `path`/`summary`" and "exactly one of `material`/`materials`" can't be
// expressed by derive alone.

impl Job {
    /// A one-off job with no materials, labor, hardware or overrides —
    /// chain the `with_*` setters and [`Job::add_material`] to fill it
    /// in. Nothing is looked up or read here.
    pub fn new(
        name: impl Into<String>,
        source: impl Into<JobSource>,
        printer: ProfileRef<PrinterProfile>,
    ) -> Self {
        Job {
            name: name.into(),
            source: source.into(),
            plate_selection: None,
            printer,
            material: HashMap::new(),
            default_material: None,
            quantity: 1,
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost::new(0.0),
            hardware: Vec::new(),
            margin_percentages_override: None,
        }
    }

    /// Costs only plate `index` (1-based) instead of every plate.
    pub fn with_plate_selection(mut self, index: Option<u32>) -> Self {
        self.plate_selection = index;
        self
    }

    /// Replaces the whole slot -> material map.
    pub fn with_materials(
        mut self,
        materials: HashMap<String, ProfileRef<MaterialProfile>>,
    ) -> Self {
        self.material = materials;
        self
    }

    /// Sets the material priced for any used slot `material` doesn't cover.
    pub fn with_default_material(mut self, material: Option<ProfileRef<MaterialProfile>>) -> Self {
        self.default_material = material;
        self
    }

    /// Sets how many units the job covers.
    pub fn with_quantity(mut self, quantity: u32) -> Self {
        self.quantity = quantity;
        self
    }

    /// Sets the labor time and rate.
    pub fn with_labor(mut self, labor: LaborCost) -> Self {
        self.labor = labor;
        self
    }

    /// Replaces the hardware list. See also [`Job::add_hardware`].
    pub fn with_hardware(mut self, hardware: Vec<HardwareItem>) -> Self {
        self.hardware = hardware;
        self
    }

    /// Overrides every material profile's own efficiency factor.
    pub fn with_material_efficiency_factor_override(mut self, factor: Option<f64>) -> Self {
        self.material_efficiency_factor_override = factor;
        self
    }

    /// Overrides the global electricity cost per kWh.
    pub fn with_electricity_cost_per_kwh_override(mut self, cost: Option<f64>) -> Self {
        self.electricity_cost_per_kwh_override = cost;
        self
    }

    /// Overrides the global margin percentages.
    pub fn with_margin_percentages_override(mut self, margins: Option<Vec<f64>>) -> Self {
        self.margin_percentages_override = margins;
        self
    }

    /// Maps `slot` (see [`crate::formats::slice_summary::FilamentUsage::slot_id`])
    /// to `material`, replacing any previous entry for that slot.
    pub fn add_material(&mut self, slot: impl Into<String>, material: ProfileRef<MaterialProfile>) {
        self.material.insert(slot.into(), material);
    }

    /// Appends a flat per-piece extra cost (inserts, magnets, ...) to this
    /// job's hardware list.
    pub fn add_hardware(&mut self, item: HardwareItem) {
        self.hardware.push(item);
    }

    /// Serializes this job to the job TOML [`crate::parse_job`] reads back,
    /// at the given [`JobTier`]:
    ///
    /// - [`JobTier::Slim`] (the default) serializes the job as-is —
    ///   `Linked` refs stay linked, `Embedded` stay embedded, the source
    ///   is untouched.
    /// - [`JobTier::Full`] embeds the printer and every material
    ///   (`Linked` refs resolved against the [`crate::builtin`] presets) —
    ///   can fail with [`Error::ProfileNotFound`]. The source is
    ///   untouched.
    /// - [`JobTier::Bundled`] additionally embeds the slice summary via
    ///   [`Job::slice_summary`] — a `Path` source is read and parsed
    ///   here, so this can also fail with [`Error::Io`]/[`Error::Parse`].
    ///
    /// Tiers only go *up* (Linked → Embedded, Path → Embedded summary):
    /// un-embedding is impossible, since a `Linked` ref's name and a
    /// `Path` source's original location are lost once resolved away.
    /// The job itself is never mutated — the embedding happens on a
    /// clone.
    pub fn to_toml(&self, tier: JobTier) -> Result<String, Error> {
        let job = match tier {
            JobTier::Slim => return toml::to_string(self).map_err(|e| Error::Toml(e.to_string())),
            JobTier::Full | JobTier::Bundled => {
                let mut job = self.clone();
                job.printer = ProfileRef::Embedded {
                    profile: Box::new(resolve_printer(&job.printer)?),
                };
                job.material = resolve_materials(&job.material)?
                    .into_iter()
                    .map(|(slot, profile)| {
                        (
                            slot,
                            ProfileRef::Embedded {
                                profile: Box::new(profile),
                            },
                        )
                    })
                    .collect();
                job.default_material = job
                    .default_material
                    .as_ref()
                    .map(|r| {
                        Ok::<_, Error>(ProfileRef::Embedded {
                            profile: Box::new(crate::resolve_material(r)?),
                        })
                    })
                    .transpose()?;
                if tier == JobTier::Bundled {
                    job.source = JobSource::Embedded(self.slice_summary()?);
                }
                job
            }
        };
        toml::to_string(&job).map_err(|e| Error::Toml(e.to_string()))
    }

    /// Parses printer profile TOML and embeds it as this job's printer —
    /// sugar for `job.printer = ProfileRef::Embedded { .. }` around
    /// [`parse_printer`]. Embedded profiles go straight through
    /// [`crate::calculate`], no built-in preset or library lookup.
    pub fn embed_printer_toml(&mut self, toml_text: &str) -> Result<(), Error> {
        self.printer = ProfileRef::Embedded {
            profile: Box::new(parse_printer(toml_text)?),
        };
        Ok(())
    }

    /// Parses filament/resin profile TOML and embeds it as the material
    /// for `slot` (see
    /// [`crate::formats::slice_summary::FilamentUsage::slot_id`]) — sugar
    /// for inserting a `ProfileRef::Embedded` into `job.material` around
    /// [`parse_material`].
    pub fn embed_material_toml(
        &mut self,
        slot: impl Into<String>,
        toml_text: &str,
    ) -> Result<(), Error> {
        self.material.insert(
            slot.into(),
            ProfileRef::Embedded {
                profile: Box::new(parse_material(toml_text)?),
            },
        );
        Ok(())
    }
}

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
    material: Option<ProfileRef<MaterialProfile>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    materials: Option<HashMap<String, ProfileRef<MaterialProfile>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_material: Option<ProfileRef<MaterialProfile>>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    material_efficiency_factor_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    electricity_cost_per_kwh_override: Option<f64>,
    labor: LaborCost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hardware: Vec<HardwareItem>,
    #[serde(
        default,
        alias = "markup_percentages_override",
        skip_serializing_if = "Option::is_none"
    )]
    margin_percentages_override: Option<Vec<f64>>,
}

/// `serde(default)` for `quantity` — a job file that doesn't mention it
/// is a single unit.
fn one() -> u32 {
    1
}

fn is_one(quantity: &u32) -> bool {
    *quantity == 1
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

        let material = match (wire.material, wire.materials) {
            (Some(single), None) => HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), single)]),
            (None, Some(map)) => map,
            // A `[default_material]` on its own covers every slot, so it
            // stands in for a per-slot map.
            (None, None) if wire.default_material.is_some() => HashMap::new(),
            (None, None) => {
                return Err(D::Error::custom(
                    "job needs `[material]`, `[materials]`, or `[default_material]`",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(D::Error::custom(
                    "job can't have both `[material]` and `[materials]`",
                ));
            }
        };

        Ok(Job {
            name: wire.name,
            source,
            plate_selection,
            printer: wire.printer,
            material,
            default_material: wire.default_material,
            quantity: wire.quantity,
            material_efficiency_factor_override: wire.material_efficiency_factor_override,
            electricity_cost_per_kwh_override: wire.electricity_cost_per_kwh_override,
            labor: wire.labor,
            hardware: wire.hardware,
            margin_percentages_override: wire.margin_percentages_override,
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

        // Only the single-slot-under-"1" shape collapses to `[material]`;
        // everything else (including a one-entry map under any other slot
        // id) is spelled out as `[materials]`.
        let (material, materials) =
            if self.material.len() == 1 && self.material.contains_key(SINGLE_MATERIAL_SLOT) {
                (self.material.get(SINGLE_MATERIAL_SLOT).cloned(), None)
            } else {
                (None, Some(self.material.clone()))
            };

        let wire = JobWire {
            name: self.name.clone(),
            source,
            printer: self.printer.clone(),
            material,
            materials,
            default_material: self.default_material.clone(),
            quantity: self.quantity,
            material_efficiency_factor_override: self.material_efficiency_factor_override,
            electricity_cost_per_kwh_override: self.electricity_cost_per_kwh_override,
            labor: self.labor,
            hardware: self.hardware.clone(),
            margin_percentages_override: self.margin_percentages_override.clone(),
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
    fn default_tier_single_material_round_trips() {
        let toml_str = r#"
name = "widget-v2"

[source]
path = "prints/widget.gcode.3mf"

[printer]
ref = "bambu-a1"

[material]
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
        assert_eq!(job.material.len(), 1);
        assert_eq!(
            job.material.get(SINGLE_MATERIAL_SLOT),
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
    fn multi_material_map_round_trips() {
        let toml_str = r#"
name = "multicolor-vase"

[source]
path = "prints/vase_ams.gcode.3mf"

[printer]
ref = "bambu-a1"

[materials]
"1" = { ref = "pla-black" }
"2" = { ref = "pla-red" }
"4" = { ref = "petg-clear" }

[labor]
rate_per_hour = 15.0
minutes = 10.0
"#;
        let job: Job = toml::from_str(toml_str).unwrap();
        assert_eq!(job.material.len(), 3);
        assert!(job.material.contains_key("4"));

        let serialized = toml::to_string(&job).unwrap();
        assert!(serialized.contains("[materials."));
        assert!(!serialized.contains("[material]"));
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
purchase_price = 800.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 80.0
power_consumption_watts = 160.0
cost_buffer_factor = 1.3

[material.profile]
name = "pla-black"
kind = "filament"
price = 18.99
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
        let material = job.material[SINGLE_MATERIAL_SLOT].embedded().unwrap();
        assert_eq!(material.kind, MaterialKind::Filament);

        let round_tripped: Job = toml::from_str(&toml::to_string(&job).unwrap()).unwrap();
        assert_eq!(round_tripped, job);
    }

    /// `add_material`/`add_hardware` build the same structures the TOML
    /// maps onto, and `to_toml` round-trips through `parse_job`'s format.
    #[test]
    fn job_helpers_build_and_round_trip() {
        let mut job: Job = toml::from_str(
            r#"
name = "widget"

[source]
path = "prints/widget.gcode.3mf"

[printer]
ref = "bambu-a1"

[material]
ref = "pla-black"

[labor]
minutes = 10.0
"#,
        )
        .unwrap();

        job.add_material(
            "2",
            ProfileRef::Linked {
                name: "pla-red".into(),
            },
        );
        assert_eq!(
            job.material.get("2"),
            Some(&ProfileRef::Linked {
                name: "pla-red".into()
            })
        );
        job.add_hardware(HardwareItem {
            name: "brass insert M3x5".into(),
            cost: 0.12,
            quantity: 4,
        });
        assert_eq!(job.hardware.len(), 1);

        let round_tripped: Job = toml::from_str(&job.to_toml(JobTier::Slim).unwrap()).unwrap();
        assert_eq!(round_tripped, job);
    }

    /// `JobSource` accepts the `new_job` source argument's either-shape:
    /// a path (`String`/`PathBuf`) or an already-parsed `SliceSummary`.
    #[test]
    fn job_source_from_impls_cover_path_and_summary() {
        assert_eq!(
            JobSource::from("prints/widget.gcode".to_string()),
            JobSource::Path("prints/widget.gcode".into())
        );
        assert_eq!(
            JobSource::from(PathBuf::from("prints/widget.gcode")),
            JobSource::Path("prints/widget.gcode".into())
        );
        let summary = SliceSummary {
            source_format: crate::formats::slice_summary::SourceFormat::Bambu3mf,
            meta: Default::default(),
            plates: vec![],
        };
        assert_eq!(
            JobSource::from(summary.clone()),
            JobSource::Embedded(summary)
        );
    }

    #[test]
    fn missing_material_and_materials_is_an_error() {
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
