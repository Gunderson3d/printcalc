//! Orchestration functions that need no filesystem: `parse_job` and
//! `calc_job_data`.
//!
//! Both `printcalc-cli` and `printcalc-wasm` call these directly rather than
//! reimplementing any of this logic themselves. Everything that touches the
//! disk — the profile library, job file I/O, config persistence — lives in
//! `printcalc-cli`, not here.

use std::collections::HashMap;
use std::fmt;

use crate::builtin;
use crate::formats::slice_summary::SliceSummary;
use crate::formats::ParseError;
use crate::model::cost::{self, CalcError, CostBreakdown, CostDefaults};
use crate::model::filament::MaterialProfile;
use crate::model::job::{Job, ProfileRef};
use crate::model::printer::PrinterProfile;

/// Why an `ops` call failed — wraps every error type the functions below
/// can hit, so callers (the CLI, the WASM bindings) only need one error
/// type to handle.
#[derive(Debug)]
#[non_exhaustive]
pub enum OpsError {
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

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpsError::Toml(e) => write!(f, "toml error: {e}"),
            OpsError::Parse(e) => write!(f, "{e}"),
            OpsError::Calc(e) => write!(f, "{e}"),
            OpsError::ProfileNotFound { kind, name } => write!(f, "{kind} {name:?} not found"),
        }
    }
}

impl std::error::Error for OpsError {}

impl From<ParseError> for OpsError {
    fn from(e: ParseError) -> Self {
        OpsError::Parse(e)
    }
}
impl From<CalcError> for OpsError {
    fn from(e: CalcError) -> Self {
        OpsError::Calc(e)
    }
}

/// Parses job TOML already in hand — no filesystem access, so this is
/// what `printcalc-wasm` uses (no filesystem to load a job file from at
/// all) and what the CLI's `load_job` calls after reading the file.
pub fn parse_job(text: &str) -> Result<Job, OpsError> {
    toml::from_str(text).map_err(|e| OpsError::Toml(e.to_string()))
}

/// Costs a `job`/`slice` already in hand: resolves its printer/filament
/// refs (`Embedded` straight through, `Linked` against the built-in
/// presets — there's no on-disk library here; the CLI resolves `Linked`
/// names against the local library before calling) and runs
/// [`cost::calculate_cost`] with the caller-supplied `defaults`.
pub fn calc_job_data(
    job: &Job,
    slice: &SliceSummary,
    defaults: &CostDefaults,
) -> Result<CostBreakdown, OpsError> {
    let printer = resolve_printer(&job.printer)?;
    let material_profiles = resolve_materials(&job.material)?;
    Ok(cost::calculate_cost(
        job,
        slice,
        &printer,
        &material_profiles,
        defaults,
    )?)
}

fn resolve_printer(printer_ref: &ProfileRef<PrinterProfile>) -> Result<PrinterProfile, OpsError> {
    match printer_ref {
        ProfileRef::Embedded { profile } => Ok((**profile).clone()),
        ProfileRef::Linked { name } => {
            builtin::printer(name)
                .cloned()
                .ok_or_else(|| OpsError::ProfileNotFound {
                    kind: "printer",
                    name: name.clone(),
                })
        }
    }
}

fn resolve_materials(
    map: &HashMap<String, ProfileRef<MaterialProfile>>,
) -> Result<HashMap<String, MaterialProfile>, OpsError> {
    map.iter()
        .map(|(slot, r)| {
            let profile = match r {
                ProfileRef::Embedded { profile } => (**profile).clone(),
                ProfileRef::Linked { name } => builtin::filament(name).cloned().ok_or_else(|| {
                    OpsError::ProfileNotFound {
                        kind: "filament",
                        name: name.clone(),
                    }
                })?,
            };
            Ok((slot.clone(), profile))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::slice_summary::{FilamentUsage, PlateSummary, SourceFormat};
    use crate::model::filament::MaterialKind;
    use crate::model::job::{JobSource, LaborCost, SINGLE_MATERIAL_SLOT};
    use crate::model::printer::{PrinterKind, WearProfile};

    /// Embedded profiles resolve straight through, and the caller-supplied
    /// `CostDefaults` (not any on-disk config) drives the fallbacks.
    #[test]
    fn calc_job_data_resolves_embedded_profiles() {
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
            source: JobSource::Embedded(slice.clone()),
            plate_selection: None,
            printer: ProfileRef::Embedded {
                profile: Box::new(PrinterProfile {
                    name: "sl1-printer".into(),
                    kind: PrinterKind::Resin,
                    wear: WearProfile {
                        purchase_price: 400.0,
                        additional_upfront_cost: 0.0,
                        estimated_life_years: 5.0,
                        estimated_uptime_percent: 0.3,
                        annual_maintenance_cost: 20.0,
                        power_consumption_watts: 60.0,
                        cost_buffer_factor: 1.2,
                    },
                }),
            },
            material: HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Embedded {
                    profile: Box::new(MaterialProfile {
                        name: "resin-grey".into(),
                        kind: MaterialKind::Resin,
                        price_per_kg: None,
                        price_per_liter: Some(40.0),
                        density_g_per_cm3: None,
                        efficiency_factor: 1.0,
                        color: None,
                        vendor: None,
                    }),
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

        let breakdown = calc_job_data(&job, &slice, &CostDefaults::default()).unwrap();

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
            OpsError::ProfileNotFound {
                kind: "printer",
                ..
            }
        ));
    }
}
