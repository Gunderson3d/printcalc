//! Orchestration functions: `new_job`, `calc_job`, `bundle_job`, and
//! add/list/show for printer & filament profiles.
//!
//! Both `printcalc-cli` and `printcalc-wasm` call these directly rather than
//! reimplementing any of this logic themselves — see the "Principle" note
//! in `SPEC.md` §1.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::formats::slice_summary::SliceSummary;
use crate::formats::{self, ParseError};
use crate::library::{self, LibraryError};
use crate::model::cost::{self, CalcError, CostBreakdown};
use crate::model::filament::FilamentProfile;
use crate::model::job::{Job, JobSource, LaborCost, ProfileRef, SINGLE_FILAMENT_SLOT};
use crate::model::printer::PrinterProfile;

#[derive(Debug)]
pub enum OpsError {
    Io(std::io::Error),
    Toml(String),
    Parse(ParseError),
    Calc(CalcError),
    Library(LibraryError),
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpsError::Io(e) => write!(f, "io error: {e}"),
            OpsError::Toml(e) => write!(f, "toml error: {e}"),
            OpsError::Parse(e) => write!(f, "{e}"),
            OpsError::Calc(e) => write!(f, "{e}"),
            OpsError::Library(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpsError {}

impl From<std::io::Error> for OpsError {
    fn from(e: std::io::Error) -> Self {
        OpsError::Io(e)
    }
}
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
impl From<LibraryError> for OpsError {
    fn from(e: LibraryError) -> Self {
        OpsError::Library(e)
    }
}

// --- printer profiles ---

pub fn add_printer(profile: &PrinterProfile) -> Result<(), OpsError> {
    Ok(library::save_printer(profile)?)
}

pub fn list_printers() -> Result<Vec<String>, OpsError> {
    Ok(library::list_printers()?)
}

pub fn show_printer(name: &str) -> Result<PrinterProfile, OpsError> {
    Ok(library::load_printer(name)?)
}

// --- filament profiles ---

pub fn add_filament(profile: &FilamentProfile) -> Result<(), OpsError> {
    Ok(library::save_filament(profile)?)
}

pub fn list_filaments() -> Result<Vec<String>, OpsError> {
    Ok(library::list_filaments()?)
}

pub fn show_filament(name: &str) -> Result<FilamentProfile, OpsError> {
    Ok(library::load_filament(name)?)
}

// --- job ---

/// Which shape to write printer/filament refs and the source in — see
/// `SPEC.md` §5. Not a field on [`Job`] itself: tier is just which
/// `ProfileRef`/`JobSource` variant gets built, chosen once at creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobTier {
    /// Printer/filament linked by name; source kept as a path re-parsed at
    /// calc time.
    Default,
    /// Printer/filament embedded; source still a path.
    Full,
    /// Printer/filament embedded; source is the parsed `SliceSummary`
    /// itself — fully self-contained, no sliced file needed at calc time.
    Bundled,
}

pub struct NewJobParams {
    pub name: String,
    pub slice_path: PathBuf,
    pub printer_name: String,
    pub filament_name: String,
    pub plate_selection: Option<u32>,
    pub labor_minutes: f64,
    pub labor_rate_per_hour: Option<f64>,
    pub tier: JobTier,
}

pub struct NewJobOutcome {
    pub job: Job,
    /// The parsed sliced file — handed back so a caller previewing the
    /// cost without saving (`SPEC.md` §7's bare `printcalc <slice>` form)
    /// doesn't need to re-read and re-parse it via [`calc_job_data`].
    pub slice: SliceSummary,
    /// `--plate` was given but the sliced file only has one plate to begin
    /// with. `SPEC.md` §7: "`--plate` is ignored (with a warning) for
    /// single-plate formats" — the CLI is responsible for the warning
    /// text, this just tells it whether to print one.
    pub plate_selection_ignored: bool,
}

/// Reads and parses `path` as a sliced file. Exposed on its own so a
/// caller (the CLI's interactive job-creation flow) can validate the file
/// before doing anything else with it — e.g. prompting for printer/
/// filament/labor — instead of discovering it's unparseable only after
/// asking the user several questions. [`new_job`] calls this internally
/// too; pass an already-parsed summary to [`new_job_from_slice`] to avoid
/// parsing the same file twice.
pub fn parse_slice(path: &Path) -> Result<SliceSummary, OpsError> {
    let bytes = fs::read(path)?;
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    Ok(formats::parse_slice_file(&bytes, filename)?)
}

/// Parses `params.slice_path` and builds a [`Job`] at the requested
/// [`JobTier`]. See [`new_job_from_slice`] for the split-out, already-
/// parsed-file version.
pub fn new_job(params: NewJobParams) -> Result<NewJobOutcome, OpsError> {
    let summary = parse_slice(&params.slice_path)?;
    new_job_from_slice(params, summary)
}

/// Same as [`new_job`], but for a sliced file already read via
/// [`parse_slice`]. `Full`/`Bundled` load the named printer/filament
/// profiles from the library to embed them (erroring if either isn't
/// saved yet) — `Default` just links by name, no library access needed
/// here. See `SPEC.md` §5, §7.
pub fn new_job_from_slice(params: NewJobParams, summary: SliceSummary) -> Result<NewJobOutcome, OpsError> {
    let plate_selection_ignored = params.plate_selection.is_some() && summary.plates.len() <= 1;
    let plate_selection = if plate_selection_ignored { None } else { params.plate_selection };

    let (printer, filament) = match params.tier {
        JobTier::Default => (
            ProfileRef::Linked { name: params.printer_name },
            ProfileRef::Linked { name: params.filament_name },
        ),
        JobTier::Full | JobTier::Bundled => (
            ProfileRef::Embedded { profile: Box::new(library::load_printer(&params.printer_name)?) },
            ProfileRef::Embedded { profile: Box::new(library::load_filament(&params.filament_name)?) },
        ),
    };
    let source = match params.tier {
        JobTier::Bundled => JobSource::Embedded(summary.clone()),
        JobTier::Default | JobTier::Full => JobSource::Path(params.slice_path),
    };

    let job = Job {
        name: params.name,
        source,
        plate_selection,
        printer,
        filament: HashMap::from([(SINGLE_FILAMENT_SLOT.to_string(), filament)]),
        material_efficiency_factor_override: None,
        electricity_cost_per_kwh_override: None,
        labor: LaborCost { rate_per_hour: params.labor_rate_per_hour, minutes: params.labor_minutes },
        hardware: Vec::new(),
        markup_percentages_override: None,
    };
    Ok(NewJobOutcome { job, slice: summary, plate_selection_ignored })
}

pub fn save_job(job: &Job, path: &Path) -> Result<(), OpsError> {
    let text = toml::to_string(job).map_err(|e| OpsError::Toml(e.to_string()))?;
    fs::write(path, text)?;
    Ok(())
}

pub fn load_job(path: &Path) -> Result<Job, OpsError> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| OpsError::Toml(e.to_string()))
}

/// Loads the job at `job_path`, resolves its printer/filament refs (against
/// the local library, for `Linked` — embedded profiles need no lookup) and
/// its slice data (re-parsed from `source.path`, or read straight off
/// `source.summary` for a Bundled job), then runs [`cost::calculate_cost`].
pub fn calc_job(job_path: &Path) -> Result<CostBreakdown, OpsError> {
    let job = load_job(job_path)?;
    let slice = resolve_slice_summary(&job, job_path)?;
    calc_job_data(&job, &slice)
}

/// Same as [`calc_job`], but for a `job`/`slice` already in hand — used to
/// preview a job's cost without writing it to disk first (`SPEC.md` §7's
/// bare `printcalc <slice>` form, no `-o`).
pub fn calc_job_data(job: &Job, slice: &SliceSummary) -> Result<CostBreakdown, OpsError> {
    let printer = resolve_printer(&job.printer)?;
    let filament_profiles = resolve_filaments(&job.filament)?;
    let global = library::load_global_config()?;
    Ok(cost::calculate_cost(job, slice, &printer, &filament_profiles, &global)?)
}

fn resolve_slice_summary(job: &Job, job_path: &Path) -> Result<SliceSummary, OpsError> {
    match &job.source {
        JobSource::Embedded(summary) => Ok(summary.clone()),
        JobSource::Path(path) => {
            let resolved = if path.is_absolute() {
                path.clone()
            } else {
                job_path.parent().unwrap_or_else(|| Path::new(".")).join(path)
            };
            let bytes = fs::read(&resolved)?;
            let filename = resolved.file_name().and_then(|f| f.to_str()).unwrap_or("");
            Ok(formats::parse_slice_file(&bytes, filename)?)
        }
    }
}

fn resolve_printer(printer_ref: &ProfileRef<PrinterProfile>) -> Result<PrinterProfile, OpsError> {
    match printer_ref {
        ProfileRef::Embedded { profile } => Ok((**profile).clone()),
        ProfileRef::Linked { name } => Ok(library::load_printer(name)?),
    }
}

fn resolve_filaments(
    map: &HashMap<String, ProfileRef<FilamentProfile>>,
) -> Result<HashMap<String, FilamentProfile>, OpsError> {
    map.iter()
        .map(|(slot, r)| {
            let profile = match r {
                ProfileRef::Embedded { profile } => (**profile).clone(),
                ProfileRef::Linked { name } => library::load_filament(name)?,
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
    use crate::model::job::HardwareItem;
    use crate::model::printer::{PrinterKind, WearProfile};

    fn sl1_fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../Testfiles/Shape-Box.sl1s")
    }

    #[test]
    fn new_job_parses_slice_and_builds_default_tier_job() {
        let outcome = new_job(NewJobParams {
            name: "test-print".into(),
            slice_path: sl1_fixture_path(),
            printer_name: "sl1-printer".into(),
            filament_name: "resin-grey".into(),
            plate_selection: None,
            labor_minutes: 10.0,
            labor_rate_per_hour: None,
            tier: JobTier::Default,
        })
        .unwrap();

        assert!(!outcome.plate_selection_ignored);
        assert_eq!(outcome.job.name, "test-print");
        assert_eq!(outcome.job.plate_selection, None);
        assert_eq!(
            outcome.job.printer,
            ProfileRef::Linked { name: "sl1-printer".into() }
        );
        assert_eq!(
            outcome.job.filament[SINGLE_FILAMENT_SLOT],
            ProfileRef::Linked { name: "resin-grey".into() }
        );
    }

    #[test]
    fn new_job_warns_when_plate_selection_is_moot() {
        // Shape-Box.sl1s has exactly one plate — requesting plate 2 has
        // nothing to select, so it should be reported as ignored.
        let outcome = new_job(NewJobParams {
            name: "test-print".into(),
            slice_path: sl1_fixture_path(),
            printer_name: "sl1-printer".into(),
            filament_name: "resin-grey".into(),
            plate_selection: Some(2),
            labor_minutes: 0.0,
            labor_rate_per_hour: None,
            tier: JobTier::Default,
        })
        .unwrap();

        assert!(outcome.plate_selection_ignored);
        assert_eq!(outcome.job.plate_selection, None);
    }

    fn temp_job_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("printcalc-ops-test-{name}.toml"))
    }

    /// Bundled-tier job (embedded summary + embedded profiles): exercises
    /// save_job/load_job and the Embedded branch of every resolve_* helper
    /// without touching the profile library at all.
    #[test]
    fn calc_job_resolves_bundled_job_from_disk() {
        let job = Job {
            name: "bundled".into(),
            source: JobSource::Embedded(SliceSummary {
                source_format: SourceFormat::PrusaSl1,
                plates: vec![PlateSummary {
                    plate_index: 1,
                    print_time_seconds: 3600.0,
                    filament_usage: vec![FilamentUsage {
                        slot_id: SINGLE_FILAMENT_SLOT.into(),
                        material_type: Some("Resin".into()),
                        color: None,
                        grams: None,
                        ml: Some(10.0),
                    }],
                    printer_model: None,
                    layer_height: None,
                    extra: HashMap::new(),
                }],
            }),
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
            filament: HashMap::from([(
                SINGLE_FILAMENT_SLOT.to_string(),
                ProfileRef::Embedded {
                    profile: Box::new(FilamentProfile {
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
            labor: LaborCost { rate_per_hour: Some(15.0), minutes: 20.0 },
            hardware: vec![HardwareItem { name: "resin vat film".into(), cost: 2.0, quantity: 1 }],
            markup_percentages_override: None,
        };

        let path = temp_job_path("bundled");
        save_job(&job, &path).unwrap();
        let breakdown = calc_job(&path).unwrap();
        let _ = fs::remove_file(&path);

        // 10ml * 1.0 factor * ($40/1000) = $0.40
        assert!((breakdown.material_cost - 0.40).abs() < 1e-9);
        // 20 minutes @ $15/hr
        assert!((breakdown.labor_cost - 5.0).abs() < 1e-9);
        assert_eq!(breakdown.hardware_cost, 2.0);
    }

    /// Default-tier job (path source + linked refs): exercises the
    /// re-parse-at-calc-time path and library-backed profile resolution.
    /// Uses `PRINTCALC_CONFIG_DIR` to keep this off the real user config —
    /// no other test in this crate reads that env var, so it's safe to set
    /// process-wide here.
    #[test]
    fn calc_job_resolves_default_tier_job_against_library() {
        let config_dir = std::env::temp_dir().join("printcalc-ops-test-config-dir");
        let _ = fs::remove_dir_all(&config_dir);
        unsafe {
            std::env::set_var("PRINTCALC_CONFIG_DIR", &config_dir);
        }

        add_printer(&PrinterProfile {
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
        })
        .unwrap();
        add_filament(&FilamentProfile {
            name: "resin-grey".into(),
            kind: MaterialKind::Resin,
            price_per_kg: None,
            price_per_liter: Some(40.0),
            density_g_per_cm3: None,
            efficiency_factor: 1.0,
            color: None,
            vendor: None,
        })
        .unwrap();
        assert_eq!(list_printers().unwrap(), vec!["sl1-printer".to_string()]);
        assert_eq!(list_filaments().unwrap(), vec!["resin-grey".to_string()]);
        assert_eq!(show_printer("sl1-printer").unwrap().kind, PrinterKind::Resin);

        let outcome = new_job(NewJobParams {
            name: "test-print".into(),
            slice_path: sl1_fixture_path(),
            printer_name: "sl1-printer".into(),
            filament_name: "resin-grey".into(),
            plate_selection: None,
            labor_minutes: 6.0,
            labor_rate_per_hour: None,
            tier: JobTier::Default,
        })
        .unwrap();

        let path = temp_job_path("default-tier");
        save_job(&outcome.job, &path).unwrap();
        let breakdown = calc_job(&path).unwrap();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&config_dir);
        unsafe {
            std::env::remove_var("PRINTCALC_CONFIG_DIR");
        }

        // Shape-Box.sl1s uses 2.598598 ml of resin.
        assert!((breakdown.material_cost - 2.598598 * (40.0 / 1000.0)).abs() < 1e-9);
    }
}
