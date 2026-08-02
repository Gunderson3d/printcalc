#![warn(missing_docs)]
//! Core library for calculating 3D print job costs from sliced-file output.
//!
//! `printcalc` is the single home for all I/O-free logic — slice-file
//! parsing, cost calculation, and job-tier data structures — plus a thin
//! read-only [`disk`] helper for loading them from files.
//! The `printcalc` binary (this same crate, `cli` feature) and
//! `printcalc-wasm` are thin frontends over it; the CLI additionally owns
//! everything write- or config-backed (the profile library, job file
//! saving, config persistence).
//!
//! # Entry points
//!
//! Everything below is re-exported at the crate root — no deep module paths
//! needed for the common cases:
//!
//! - [`parse_slice_bytes`] turns sliced-file bytes into a [`SliceSummary`];
//!   [`read_slice_file`] does the same straight from a path.
//! - [`parse_job`] parses job TOML into a [`Job`]; [`read_job`] reads it
//!   from a file. [`parse_printer`]/[`parse_material`] parse profile TOML;
//!   [`read_printer`]/[`read_material`] load the same from disk.
//! - [`new_job`] builds a [`Job`] from a sliced-file path (or an
//!   already-parsed [`SliceSummary`]) and an embedded printer profile —
//!   no lookups, can't fail.
//! - [`calculate`] costs a job, pulling its slice data via
//!   `Job::slice_summary` and resolving `Embedded` profile refs straight
//!   through and `Linked` ones against the [`builtin`] presets. All of the
//!   above return the single [`Error`] type (except [`parse_slice_bytes`],
//!   which returns [`ParseError`]).
//! - [`calculate_cost`] is the pure core computation for callers supplying
//!   fully-resolved profiles themselves (as `printcalc-wasm` does, with no
//!   filesystem to read from).
//!
//! The example below shows the `calculate_cost` path — the core computation
//! with no I/O at all, since that's the part that needs no real files on
//! disk to demonstrate:
//!
//! ```
//! use std::collections::HashMap;
//!
//! use printcalc::{
//!     calculate_cost, CostDefaults, FilamentUsage, Job, JobSource, LaborCost, MaterialKind,
//!     MaterialProfile, PlateSummary, PrinterKind, PrinterProfile, ProfileRef, SliceSummary,
//!     SourceFormat, WearProfile, SINGLE_MATERIAL_SLOT,
//! };
//!
//! // Normally the output of `printcalc::parse_slice_bytes(bytes, filename)`.
//! let slice = SliceSummary {
//!     source_format: SourceFormat::Bambu3mf,
//!     plates: vec![PlateSummary {
//!         plate_index: 1,
//!         print_time_seconds: 1037.0,
//!         filament_usage: vec![FilamentUsage {
//!             slot_id: SINGLE_MATERIAL_SLOT.to_string(),
//!             material_type: Some("PLA".to_string()),
//!             color: None,
//!             grams: Some(6.66),
//!             ml: None,
//!         }],
//!         printer_model: None,
//!         layer_height: None,
//!         extra: HashMap::new(),
//!     }],
//! };
//!
//! let printer = PrinterProfile {
//!     name: "bambu-a1".to_string(),
//!     kind: PrinterKind::Fdm,
//!     wear: WearProfile {
//!         purchase_price: 800.0,
//!         additional_upfront_cost: 0.0,
//!         estimated_life_years: 5.0,
//!         estimated_uptime_percent: 0.5,
//!         annual_maintenance_cost: 80.0,
//!         power_consumption_watts: 160.0,
//!         cost_buffer_factor: 1.3,
//!     },
//! };
//! let filament = MaterialProfile {
//!     name: "pla-black".to_string(),
//!     kind: MaterialKind::Filament,
//!     price: 18.99,
//!     efficiency_factor: 1.1,
//!     color: None,
//!     vendor: None,
//! };
//!
//! let job = Job {
//!     name: "widget".to_string(),
//!     source: JobSource::Embedded(slice.clone()),
//!     plate_selection: None,
//!     printer: ProfileRef::Linked { name: "bambu-a1".to_string() },
//!     material: HashMap::from([(
//!         SINGLE_MATERIAL_SLOT.to_string(),
//!         ProfileRef::Linked { name: "pla-black".to_string() },
//!     )]),
//!     material_efficiency_factor_override: None,
//!     electricity_cost_per_kwh_override: None,
//!     labor: LaborCost { rate_per_hour: Some(15.0), minutes: 10.0 },
//!     hardware: vec![],
//!     markup_percentages_override: None,
//! };
//!
//! // In a real app, the caller resolves `job.printer`/`job.material`
//! // (which may be `Linked` refs) first; here they're already resolved,
//! // so `calculate_cost` is called directly.
//! let material_profiles = HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), filament)]);
//! let breakdown =
//!     calculate_cost(&job, &slice, &printer, &material_profiles, &CostDefaults::default())
//!         .expect("printer/filament kinds match the sliced file, and every used slot is priced");
//! assert!(breakdown.total_cost > 0.0);
//! ```

use std::collections::HashMap;
use std::fmt;

pub mod builtin;
pub mod disk;
pub mod formats;
pub mod model;

pub use disk::{read_job, read_material, read_printer, read_slice_file};
pub use formats::ParseError;
pub use formats::parse_slice_bytes;
pub use formats::slice_summary::{FilamentUsage, PlateSummary, SliceSummary, SourceFormat};
pub use model::cost::{CalcError, CostBreakdown, CostDefaults, calculate_cost};
pub use model::filament::{MaterialKind, MaterialProfile};
pub use model::job::{
    HardwareItem, Job, JobSource, JobTier, LaborCost, ProfileRef, SINGLE_MATERIAL_SLOT,
};
pub use model::printer::{PrinterKind, PrinterProfile, WearProfile};

use model::cost;

/// Why a crate-root call failed — wraps every error type the functions
/// below can hit, so callers (the CLI, the WASM bindings) only need one
/// error type to handle.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A file read failed (only the [`disk`] helpers touch the filesystem).
    Io(std::io::Error),
    /// Job TOML (de)serialization failed; the message is `toml`'s own
    /// error text.
    Toml(String),
    /// Parsing a sliced file failed.
    Parse(ParseError),
    /// Costing a job failed.
    Calc(CalcError),
    /// A `Linked` profile ref named neither an embedded profile nor a
    /// built-in preset (there's no on-disk library at this level — that's
    /// the CLI's job to resolve first).
    ProfileNotFound {
        /// `"printer"` or `"filament"`.
        kind: &'static str,
        /// The name that was looked up.
        name: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Toml(e) => write!(f, "toml error: {e}"),
            Error::Parse(e) => write!(f, "{e}"),
            Error::Calc(e) => write!(f, "{e}"),
            Error::ProfileNotFound { kind, name } => write!(f, "{kind} {name:?} not found"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}
impl From<CalcError> for Error {
    fn from(e: CalcError) -> Self {
        Error::Calc(e)
    }
}

/// Parses job TOML already in hand — no filesystem access, so this is
/// what `printcalc-wasm` uses (no filesystem to load a job file from at
/// all) and what the CLI's `load_job` calls after reading the file.
pub fn parse_job(text: &str) -> Result<Job, Error> {
    toml::from_str(text).map_err(|e| Error::Toml(e.to_string()))
}

/// Parses printer profile TOML already in hand — the same format
/// [`read_printer`] loads from disk and the CLI saves into its
/// profile library.
pub fn parse_printer(text: &str) -> Result<PrinterProfile, Error> {
    toml::from_str(text).map_err(|e| Error::Toml(e.to_string()))
}

/// Parses filament/resin profile TOML already in hand — the same format
/// [`read_material`] loads from disk and the CLI saves into its
/// profile library.
pub fn parse_material(text: &str) -> Result<MaterialProfile, Error> {
    toml::from_str(text).map_err(|e| Error::Toml(e.to_string()))
}

/// Builds a printer profile from its parts — the same shape
/// [`parse_printer`] produces from TOML.
pub fn create_printer(
    name: impl Into<String>,
    kind: PrinterKind,
    wear: WearProfile,
) -> PrinterProfile {
    PrinterProfile {
        name: name.into(),
        kind,
        wear,
    }
}

/// Builds a material profile from its parts — the same shape
/// [`parse_material`] produces from TOML. `price` is per kilogram for
/// [`MaterialKind::Filament`], per liter for [`MaterialKind::Resin`].
#[allow(clippy::too_many_arguments)]
pub fn create_material(
    name: impl Into<String>,
    kind: MaterialKind,
    price: f64,
    efficiency_factor: f64,
    color: Option<String>,
    vendor: Option<String>,
) -> MaterialProfile {
    MaterialProfile {
        name: name.into(),
        kind,
        price,
        efficiency_factor,
        color,
        vendor,
    }
}

/// Builds a [`Job`] with the printer embedded by value (no library or
/// built-in preset lookup at calc time) and `materials` keyed by
/// [`FilamentUsage::slot_id`] — the single-material case is one entry
/// under [`SINGLE_MATERIAL_SLOT`]. `source` is either a sliced-file path
/// (re-parsed at calc time) or an already-parsed [`SliceSummary`]
/// embedded directly. The remaining fields default (`plate_selection:
/// None`, no overrides), and `labor`/`hardware` may be
/// `None`/empty. Nothing is looked up or read here, so this can't fail.
pub fn new_job(
    name: impl Into<String>,
    source: impl Into<JobSource>,
    printer: PrinterProfile,
    materials: HashMap<String, ProfileRef<MaterialProfile>>,
    labor: Option<LaborCost>,
    hardware: Vec<HardwareItem>,
) -> Job {
    Job {
        name: name.into(),
        source: source.into(),
        plate_selection: None,
        printer: ProfileRef::Embedded {
            profile: Box::new(printer),
        },
        material: materials,
        material_efficiency_factor_override: None,
        electricity_cost_per_kwh_override: None,
        labor: labor.unwrap_or(LaborCost {
            rate_per_hour: None,
            minutes: 0.0,
        }),
        hardware,
        markup_percentages_override: None,
    }
}

/// Costs a `job` end to end: pulls its slice data via
/// [`Job::slice_summary`] (cloned from an `Embedded` source, or re-parsed
/// from a `Path` source), resolves its printer/material refs (`Embedded`
/// straight through, `Linked` against the built-in presets — there's no
/// on-disk library here; the CLI resolves `Linked` names against the
/// local library before calling) and runs [`calculate_cost`] with the
/// caller-supplied `defaults`.
///
/// Takes `&Job` rather than the `job: Job` of `Api.md` — nothing here
/// needs ownership, and borrowing lets callers reuse the job afterwards.
pub fn calculate(job: &Job, defaults: &CostDefaults) -> Result<CostBreakdown, Error> {
    let slice = job.slice_summary()?;
    let printer = resolve_printer(&job.printer)?;
    let material_profiles = resolve_materials(&job.material)?;
    Ok(cost::calculate_cost(
        job,
        &slice,
        &printer,
        &material_profiles,
        defaults,
    )?)
}

impl Job {
    /// Serializes this job to the job TOML [`parse_job`] reads back, at
    /// the given [`JobTier`]:
    ///
    /// - [`JobTier::Slim`] (the default) serializes the job as-is —
    ///   `Linked` refs stay linked, `Embedded` stay embedded, the source
    ///   is untouched.
    /// - [`JobTier::Full`] embeds the printer and every material
    ///   (`Linked` refs resolved against the [`builtin`] presets) — can
    ///   fail with [`Error::ProfileNotFound`]. The source is untouched.
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
    /// [`calculate`], no built-in preset or library lookup.
    pub fn embed_printer_toml(&mut self, toml_text: &str) -> Result<(), Error> {
        self.printer = ProfileRef::Embedded {
            profile: Box::new(parse_printer(toml_text)?),
        };
        Ok(())
    }

    /// Parses filament/resin profile TOML and embeds it as the material
    /// for `slot` (see [`FilamentUsage::slot_id`]) — sugar for inserting a
    /// `ProfileRef::Embedded` into `job.material` around
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

fn resolve_printer(printer_ref: &ProfileRef<PrinterProfile>) -> Result<PrinterProfile, Error> {
    match printer_ref {
        ProfileRef::Embedded { profile } => Ok((**profile).clone()),
        ProfileRef::Linked { name } => {
            builtin::printer(name)
                .cloned()
                .ok_or_else(|| Error::ProfileNotFound {
                    kind: "printer",
                    name: name.clone(),
                })
        }
    }
}

fn resolve_materials(
    map: &HashMap<String, ProfileRef<MaterialProfile>>,
) -> Result<HashMap<String, MaterialProfile>, Error> {
    map.iter()
        .map(|(slot, r)| {
            let profile = match r {
                ProfileRef::Embedded { profile } => (**profile).clone(),
                ProfileRef::Linked { name } => {
                    builtin::filament(name)
                        .cloned()
                        .ok_or_else(|| Error::ProfileNotFound {
                            kind: "filament",
                            name: name.clone(),
                        })?
                }
            };
            Ok((slot.clone(), profile))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_wear() -> WearProfile {
        WearProfile {
            purchase_price: 500.0,
            additional_upfront_cost: 0.0,
            estimated_life_years: 5.0,
            estimated_uptime_percent: 0.5,
            annual_maintenance_cost: 50.0,
            power_consumption_watts: 150.0,
            cost_buffer_factor: 1.2,
        }
    }

    /// Embedded profiles resolve straight through, and the caller-supplied
    /// `CostDefaults` (not any on-disk config) drives the fallbacks.
    #[test]
    fn calculate_resolves_embedded_profiles() {
        let slice = SliceSummary {
            source_format: SourceFormat::PrusaSl1,
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 3600.0,
                filament_usage: vec![FilamentUsage {
                    slot_id: SINGLE_MATERIAL_SLOT.into(),
                    material_type: Some("Resin".into()),
                    color: None,
                    grams: None,
                    ml: Some(10.0),
                }],
                printer_model: None,
                layer_height: None,
                extra: HashMap::new(),
            }],
        };
        let job = Job {
            name: "bundled".into(),
            source: JobSource::Embedded(slice),
            plate_selection: None,
            printer: ProfileRef::Embedded {
                profile: Box::new(create_printer(
                    "sl1-printer",
                    PrinterKind::Resin,
                    WearProfile {
                        purchase_price: 400.0,
                        estimated_uptime_percent: 0.3,
                        ..test_wear()
                    },
                )),
            },
            material: HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Embedded {
                    profile: Box::new(create_material(
                        "resin-grey",
                        MaterialKind::Resin,
                        40.0,
                        1.0,
                        None,
                        None,
                    )),
                },
            )]),
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost {
                rate_per_hour: None, // falls back to the CostDefaults labor rate
                minutes: 20.0,
            },
            hardware: Vec::new(),
            markup_percentages_override: None,
        };

        let breakdown = calculate(&job, &CostDefaults::default()).unwrap();

        // 10ml * 1.0 factor * ($40/1000) = $0.40
        assert!((breakdown.material_cost - 0.40).abs() < 1e-9);
        // 20 minutes @ the default $15/hr
        assert!((breakdown.labor_cost - 5.0).abs() < 1e-9);
    }

    /// A `Linked` ref naming no built-in preset is a `ProfileNotFound` —
    /// resolving against an on-disk library is the CLI's job, not this
    /// layer's.
    #[test]
    fn linked_ref_to_unknown_profile_is_not_found() {
        let err = resolve_printer(&ProfileRef::Linked {
            name: "no-such-printer".into(),
        })
        .unwrap_err();
        assert!(matches!(
            err,
            Error::ProfileNotFound {
                kind: "printer",
                ..
            }
        ));
    }

    /// Profile TOML in hand parses into profiles; malformed TOML is an
    /// `Error::Toml`.
    #[test]
    fn parse_profiles_from_toml_text() {
        let printer = parse_printer(
            r#"
name = "test-printer"
kind = "fdm"
purchase_price = 500.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 50.0
power_consumption_watts = 150.0
cost_buffer_factor = 1.2
"#,
        )
        .unwrap();
        assert_eq!(printer.kind, PrinterKind::Fdm);

        let material = parse_material(
            r#"
name = "test-pla"
kind = "filament"
price = 20.0
efficiency_factor = 1.1
"#,
        )
        .unwrap();
        assert_eq!(material.kind, MaterialKind::Filament);

        assert!(matches!(
            parse_material("not = [valid").unwrap_err(),
            Error::Toml(_)
        ));
    }

    /// `create_printer` builds the same shape TOML parses into, with the
    /// wear keys flattened to the profile's top level (no `[wear]`
    /// table), and `to_toml` round-trips through `parse_printer`.
    #[test]
    fn printer_profile_toml_round_trips_with_flattened_wear() {
        let printer = create_printer("test-printer", PrinterKind::Fdm, test_wear());

        let text = printer.to_toml().unwrap();
        assert!(!text.contains("[wear]"));
        assert!(text.contains("purchase_price = 500.0"));

        let round_tripped = parse_printer(&text).unwrap();
        assert_eq!(round_tripped, printer);
        assert_eq!(round_tripped.wear.purchase_price, 500.0);
    }

    /// `create_material` sets exactly the fields it's given, and the
    /// profile's `to_toml` round-trips through `parse_material`.
    #[test]
    fn material_profile_toml_round_trips() {
        let material = create_material(
            "test-pla",
            MaterialKind::Filament,
            20.0,
            1.1,
            Some("black".to_string()),
            None,
        );
        assert_eq!(material.price, 20.0);
        assert_eq!(material.color.as_deref(), Some("black"));
        assert_eq!(material.vendor, None);

        let round_tripped = parse_material(&material.to_toml().unwrap()).unwrap();
        assert_eq!(round_tripped, material);
    }

    /// Profile TOML strings embedded via the `Job` methods cost straight
    /// through — names that match no built-in preset, no lookup involved.
    #[test]
    fn embed_toml_profiles_make_job_costable_without_presets() {
        let slice_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Testfiles/Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode");
        let mut job = new_job(
            "widget",
            slice_path.to_string_lossy().into_owned(),
            create_printer("ignored", PrinterKind::Fdm, test_wear()),
            HashMap::new(),
            None,
            Vec::new(),
        );
        job.embed_printer_toml(
            r#"
name = "custom-printer"
kind = "fdm"
purchase_price = 500.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 50.0
power_consumption_watts = 150.0
cost_buffer_factor = 1.2
"#,
        )
        .unwrap();
        job.embed_material_toml(
            SINGLE_MATERIAL_SLOT,
            r#"
name = "custom-pla"
kind = "filament"
price = 20.0
efficiency_factor = 1.1
"#,
        )
        .unwrap();

        assert!(matches!(job.printer, ProfileRef::Embedded { .. }));
        assert!(matches!(
            job.material[SINGLE_MATERIAL_SLOT],
            ProfileRef::Embedded { .. }
        ));

        let breakdown = calculate(&job, &CostDefaults::default()).unwrap();
        assert!(breakdown.total_cost > 0.0);

        assert!(matches!(
            job.embed_printer_toml("not = [valid").unwrap_err(),
            Error::Toml(_)
        ));
    }

    /// `new_job` embeds the printer by value, TOML round-trips, and costs
    /// end to end against a real sliced file via its `Path` source (the
    /// linked material resolves against the built-in presets).
    #[test]
    fn new_job_builds_costable_job_from_path_source() {
        let slice_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Testfiles/Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode");
        let job = new_job(
            "widget",
            slice_path.to_string_lossy().into_owned(),
            create_printer("bambu-a1", PrinterKind::Fdm, test_wear()),
            HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Linked {
                    name: "pla-black".into(),
                },
            )]),
            Some(LaborCost {
                rate_per_hour: None,
                minutes: 10.0,
            }),
            Vec::new(),
        );

        assert_eq!(job.source, JobSource::Path(slice_path.clone()));
        assert_eq!(
            job.printer.embedded().map(|p| p.name.as_str()),
            Some("bambu-a1")
        );

        let round_tripped = parse_job(&job.to_toml(JobTier::Slim).unwrap()).unwrap();
        assert_eq!(round_tripped, job);

        let breakdown = calculate(&job, &CostDefaults::default()).unwrap();
        assert!(breakdown.total_cost > 0.0);
        assert!((breakdown.labor_cost - 2.5).abs() < 1e-9);
    }

    /// `new_job` also accepts an already-parsed `SliceSummary` as its
    /// source — the fully self-contained shape that needs no sliced file
    /// at calc time.
    #[test]
    fn new_job_with_embedded_summary_costs_without_a_file() {
        let slice_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Testfiles/Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode");
        let summary = read_slice_file(&slice_path).unwrap();
        let job = new_job(
            "widget",
            summary,
            create_printer("bambu-a1", PrinterKind::Fdm, test_wear()),
            HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Linked {
                    name: "pla-black".into(),
                },
            )]),
            None,
            Vec::new(),
        );

        assert!(matches!(job.source, JobSource::Embedded(_)));
        let breakdown = calculate(&job, &CostDefaults::default()).unwrap();
        assert!(breakdown.total_cost > 0.0);
    }

    /// A linked job (refs by name, path source) for the `to_toml` tier
    /// tests — vendor-neutral preset names only, so it resolves under any
    /// feature set.
    fn linked_job() -> Job {
        Job {
            name: "widget".into(),
            source: JobSource::Path("prints/widget.gcode".into()),
            plate_selection: None,
            printer: ProfileRef::Linked {
                name: "generic-resin".into(),
            },
            material: HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Linked {
                    name: "generic-resin-grey".into(),
                },
            )]),
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost {
                rate_per_hour: None,
                minutes: 10.0,
            },
            hardware: Vec::new(),
            markup_percentages_override: None,
        }
    }

    /// Slim serializes as-is: refs stay linked, the wire format is the
    /// same shape hand-written job files use, and it round-trips.
    #[test]
    fn to_toml_slim_keeps_linked_refs() {
        let job = linked_job();

        let text = job.to_toml(JobTier::Slim).unwrap();
        assert!(text.contains("ref = \"generic-resin\""));
        assert!(!text.contains("[printer.profile]"));

        let round_tripped = parse_job(&text).unwrap();
        assert_eq!(round_tripped, job);
    }

    /// Full embeds printer and materials (resolved against the built-in
    /// presets) but leaves the source alone — and doesn't mutate the job.
    #[test]
    fn to_toml_full_embeds_profiles_but_not_source() {
        let job = linked_job();

        let full = parse_job(&job.to_toml(JobTier::Full).unwrap()).unwrap();
        assert_eq!(
            full.printer.embedded().map(|p| p.name.as_str()),
            Some("generic-resin")
        );
        assert!(matches!(
            full.material[SINGLE_MATERIAL_SLOT],
            ProfileRef::Embedded { .. }
        ));
        assert_eq!(full.source, JobSource::Path("prints/widget.gcode".into()));

        // The job itself is untouched — tiers are a serialization choice.
        assert!(matches!(job.printer, ProfileRef::Linked { .. }));
    }

    /// Bundled additionally embeds the slice summary — a `Path` source is
    /// read and parsed at serialization time.
    #[test]
    fn to_toml_bundled_embeds_profiles_and_summary() {
        let slice_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Testfiles/Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode");
        let job = Job {
            source: JobSource::Path(slice_path),
            ..linked_job()
        };

        let bundled = parse_job(&job.to_toml(JobTier::Bundled).unwrap()).unwrap();
        assert!(matches!(bundled.printer, ProfileRef::Embedded { .. }));
        let JobSource::Embedded(summary) = &bundled.source else {
            panic!("expected an embedded summary");
        };
        assert!(!summary.plates.is_empty());
    }

    /// Full/Bundled can't embed a `Linked` ref naming no built-in preset
    /// (there's no on-disk library at this level).
    #[test]
    fn to_toml_full_and_bundled_fail_on_unknown_linked_profile() {
        let mut job = linked_job();
        job.printer = ProfileRef::Linked {
            name: "no-such-printer".into(),
        };

        for tier in [JobTier::Full, JobTier::Bundled] {
            assert!(matches!(
                job.to_toml(tier).unwrap_err(),
                Error::ProfileNotFound {
                    kind: "printer",
                    ..
                }
            ));
        }
    }
}
