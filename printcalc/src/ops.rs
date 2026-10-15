//! Disk-backed orchestration functions: `new_job`, `calc_job`, `retier_job`,
//! config show/set, and add/list/show for printer & filament profiles.
//!
//! Everything here touches the filesystem (the profile library, job files,
//! `config.toml`) — the filesystem-free counterparts (`parse_job`,
//! in-memory calc) live at the `printcalc` root.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use libprintcalc::builtin;
use libprintcalc::formats::slice_summary::SliceSummary;
use libprintcalc::formats::{self, ParseError};
use libprintcalc::model::cost::{self, CalcError, CostBreakdown, CostDefaults};
use libprintcalc::model::filament::MaterialProfile;
use libprintcalc::model::job::{HardwareItem, Job, JobSource, LaborCost, ProfileRef};
use libprintcalc::model::printer::PrinterProfile;

use crate::disk::{self, LibraryError};

/// Why an `ops` call failed — wraps every error type the functions below
/// can hit, so callers only need one error type to handle.
#[derive(Debug)]
#[non_exhaustive]
pub enum OpsError {
    /// A file read/write failed.
    Io(std::io::Error),
    /// Job TOML (de)serialization failed; the message is `toml`'s own
    /// error text.
    Toml(String),
    /// Parsing a sliced file failed.
    Parse(ParseError),
    /// Costing a job failed.
    Calc(CalcError),
    /// A profile library or config operation failed.
    Library(LibraryError),
    /// A filesystem-free `printcalc` call failed.
    Lib(libprintcalc::Error),
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpsError::Io(e) => write!(f, "io error: {e}"),
            OpsError::Toml(e) => write!(f, "toml error: {e}"),
            OpsError::Parse(e) => write!(f, "{e}"),
            OpsError::Calc(e) => write!(f, "{e}"),
            OpsError::Library(e) => write!(f, "{e}"),
            OpsError::Lib(e) => write!(f, "{e}"),
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
impl From<libprintcalc::Error> for OpsError {
    fn from(e: libprintcalc::Error) -> Self {
        OpsError::Lib(e)
    }
}

/// Loads the saved config — overrides layered over
/// [`CostDefaults::default`] (see [`disk::load_config`]).
pub fn show_config() -> Result<CostDefaults, OpsError> {
    Ok(disk::load_config()?)
}

/// Persists the config, overwriting any previously saved one.
pub fn set_config(config: &CostDefaults) -> Result<(), OpsError> {
    Ok(disk::save_config(config)?)
}

/// Saves a printer profile to the local library, overwriting any existing
/// one under the same name.
pub fn add_printer(profile: &PrinterProfile) -> Result<(), OpsError> {
    Ok(disk::save_printer(profile)?)
}

/// Every printer profile name — saved ones plus any built-in preset (see
/// [`libprintcalc::builtin`]) not already saved under the same name,
/// sorted.
pub fn list_printers() -> Result<Vec<String>, OpsError> {
    let mut names = disk::list_printers()?;
    for profile in builtin::printers() {
        if !names.contains(&profile.name) {
            names.push(profile.name.clone());
        }
    }
    names.sort();
    Ok(names)
}

/// Loads a printer profile by name — checks the local library first, then
/// falls back to a built-in preset (see [`libprintcalc::builtin`]) under
/// the same name.
pub fn show_printer(name: &str) -> Result<PrinterProfile, OpsError> {
    match disk::load_printer(name) {
        Ok(profile) => Ok(profile),
        Err(LibraryError::NotFound { .. }) => builtin::printer(name).cloned().ok_or_else(|| {
            OpsError::Library(LibraryError::NotFound {
                kind: "printer",
                name: name.to_string(),
            })
        }),
        Err(e) => Err(e.into()),
    }
}

/// Saves a filament profile to the local library, overwriting any existing
/// one under the same name.
pub fn add_filament(profile: &MaterialProfile) -> Result<(), OpsError> {
    Ok(disk::save_filament(profile)?)
}

/// Every filament profile name — saved ones plus any built-in preset (see
/// [`libprintcalc::builtin`]) not already saved under the same name,
/// sorted.
pub fn list_filaments() -> Result<Vec<String>, OpsError> {
    let mut names = disk::list_filaments()?;
    for profile in builtin::filaments() {
        if !names.contains(&profile.name) {
            names.push(profile.name.clone());
        }
    }
    names.sort();
    Ok(names)
}

/// Loads a filament profile by name — checks the local library first, then
/// falls back to a built-in preset (see [`libprintcalc::builtin`]) under
/// the same name.
pub fn show_filament(name: &str) -> Result<MaterialProfile, OpsError> {
    match disk::load_filament(name) {
        Ok(profile) => Ok(profile),
        Err(LibraryError::NotFound { .. }) => builtin::filament(name).cloned().ok_or_else(|| {
            OpsError::Library(LibraryError::NotFound {
                kind: "filament",
                name: name.to_string(),
            })
        }),
        Err(e) => Err(e.into()),
    }
}

/// Which shape to write printer/filament refs and the source in. Not a
/// field on [`Job`] itself: tier is just which `ProfileRef`/`JobSource`
/// variant gets built, chosen once at creation.
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

/// Which profile to price something against: a name to look up in the
/// library (or the built-ins), or a profile already in hand.
///
/// The second case is what auto-detection produces — a profile built out
/// of a sliced file's own stated filament price has no library entry to
/// name, so it's carried by value and embedded whatever the job tier.
#[derive(Debug, Clone)]
pub enum ProfileChoice<T> {
    /// Looked up via [`show_printer`]/[`show_filament`].
    Named(String),
    /// Used as-is, no lookup.
    Profile(Box<T>),
}

impl<T> From<String> for ProfileChoice<T> {
    fn from(name: String) -> Self {
        ProfileChoice::Named(name)
    }
}

/// Inputs to [`new_job`].
pub struct NewJobParams {
    /// A free-form label for the job.
    pub name: String,
    /// The sliced file to parse and cost against.
    pub slice_path: PathBuf,
    /// Which printer profile to price wear and electricity against.
    pub printer: ProfileChoice<PrinterProfile>,
    /// slot_id -> filament profile. The common single-material case
    /// is just a one-entry map — building that one entry under the right
    /// slot_id (not always `"1"`, see
    /// [`SliceSummary::distinct_filament_slots`])
    /// is the caller's job, same as `[material]` vs `[materials]` is just
    /// two TOML shapes for the same underlying map.
    pub filaments: HashMap<String, ProfileChoice<MaterialProfile>>,
    /// `None` => every plate; `Some(i)` => just plate `i`.
    pub plate_selection: Option<u32>,
    /// Time spent on labor for this job.
    pub labor_minutes: f64,
    /// `None` => fall back to the config default.
    pub labor_rate_per_hour: Option<f64>,
    /// Filament for any used slot `filaments` doesn't cover. `None` =>
    /// every used slot must be covered explicitly.
    pub default_filament: Option<ProfileChoice<MaterialProfile>>,
    /// How many units the job covers; `1` for a one-off.
    pub quantity: u32,
    /// Flat per-unit extras (inserts, magnets, packaging, ...).
    pub hardware: Vec<HardwareItem>,
    /// Which shape to build the job in.
    pub tier: JobTier,
}

/// Result of [`new_job`].
pub struct NewJobOutcome {
    /// The built job.
    pub job: Job,
    /// The parsed sliced file — handed back so a caller previewing the
    /// cost without saving (the bare `printcalc <slice>` form) doesn't
    /// need to re-read and re-parse it via [`calc_job_data`].
    pub slice: SliceSummary,
    /// `--plate` was given but the sliced file only has one plate to begin
    /// with, so it's ignored — the CLI is responsible for the warning
    /// text, this just tells it whether to print one.
    pub plate_selection_ignored: bool,
}

/// What a sliced file identified about itself, matched against this
/// machine's profile library and the built-in presets. Returned by
/// [`detect_profiles`].
pub struct Detected {
    /// The printer the file names, if a profile matched it.
    pub printer: Option<libprintcalc::Match<PrinterProfile>>,
    /// One entry per used filament slot, in slot order.
    pub slots: Vec<libprintcalc::AutoSlot>,
}

/// Matches the printer and each filament slot a sliced file names against
/// the saved profile library first, then the built-in presets, falling
/// back per slot to a profile built from the file's own stated filament
/// price.
///
/// Purely advisory — nothing is written and nothing is decided here. The
/// CLI shows the result and lets `--printer`/`--filament`/`--filament-map`
/// override any of it.
pub fn detect_profiles(slice: &SliceSummary) -> Detected {
    // The library is a nicety here: if it can't be read, the built-in
    // presets alone still give a useful answer.
    let printers: Vec<PrinterProfile> = disk::list_printers()
        .unwrap_or_default()
        .iter()
        .filter_map(|name| disk::load_printer(name).ok())
        .collect();
    let filaments: Vec<MaterialProfile> = disk::list_filaments()
        .unwrap_or_default()
        .iter()
        .filter_map(|name| disk::load_filament(name).ok())
        .collect();
    let efficiency_factor = default_efficiency_factor(&filaments);

    let auto = libprintcalc::auto_job(
        "detect",
        slice.clone(),
        slice.clone(),
        printers.iter(),
        filaments.iter(),
        efficiency_factor,
    );
    Detected {
        printer: auto.printer,
        slots: auto.slots,
    }
}

/// Waste allowance for a profile built from a sliced file's own price.
/// No slicer records one, so this borrows the median of whatever the
/// user's saved filaments use, falling back to the documented default
/// when they have none saved.
fn default_efficiency_factor(saved: &[MaterialProfile]) -> f64 {
    let mut factors: Vec<f64> = saved
        .iter()
        .map(|m| m.efficiency_factor)
        .filter(|f| f.is_finite() && *f > 0.0)
        .collect();
    if factors.is_empty() {
        return libprintcalc::model::filament::DEFAULT_EFFICIENCY_FACTOR;
    }
    factors.sort_by(|a, b| a.partial_cmp(b).expect("filtered to finite values"));
    factors[factors.len() / 2]
}

/// Reads and parses `path` as a sliced file. Exposed on its own so a
/// caller (the CLI's interactive job-creation flow) can validate the file
/// before doing anything else with it — e.g. prompting for printer/
/// filament/labor — instead of discovering it's unparseable only after
/// asking the user several questions. Its result is then handed to
/// [`new_job`], so the file is only ever parsed once.
pub fn parse_slice(path: &Path) -> Result<SliceSummary, OpsError> {
    let bytes = fs::read(path)?;
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    Ok(formats::parse_slice_bytes(&bytes, filename)?)
}

/// Builds a [`Job`] at the requested [`JobTier`] from a sliced file
/// already read via [`parse_slice`].
///
/// `Full`/`Bundled` load the named printer/filament profiles (library or
/// built-in, see [`show_printer`]/[`show_filament`]) to embed them,
/// erroring if a name is neither saved nor built-in. `Default` just links
/// by name, so no lookup happens here.
pub fn new_job(params: NewJobParams, summary: SliceSummary) -> Result<NewJobOutcome, OpsError> {
    let plate_selection_ignored = params.plate_selection.is_some() && summary.plates.len() <= 1;
    let plate_selection = if plate_selection_ignored {
        None
    } else {
        params.plate_selection
    };

    let printer = resolve_choice(params.printer, params.tier, show_printer)?;
    let material: HashMap<String, ProfileRef<MaterialProfile>> = params
        .filaments
        .into_iter()
        .map(|(slot, choice)| Ok((slot, resolve_choice(choice, params.tier, show_filament)?)))
        .collect::<Result<_, OpsError>>()?;
    let default_material = params
        .default_filament
        .map(|choice| resolve_choice(choice, params.tier, show_filament))
        .transpose()?;
    let source = match params.tier {
        JobTier::Bundled => JobSource::Embedded(summary.clone()),
        JobTier::Default | JobTier::Full => JobSource::Path(params.slice_path),
    };

    let job = Job {
        name: params.name,
        source,
        plate_selection,
        printer,
        material,
        default_material,
        quantity: params.quantity,
        material_efficiency_factor_override: None,
        electricity_cost_per_kwh_override: None,
        labor: LaborCost {
            rate_per_hour: params.labor_rate_per_hour,
            minutes: params.labor_minutes,
        },
        hardware: params.hardware,
        margin_percentages_override: None,
    };
    Ok(NewJobOutcome {
        job,
        slice: summary,
        plate_selection_ignored,
    })
}

/// Writes `job` as TOML to `path`, overwriting any existing file there.
/// Serialized at the lib's [`libprintcalc::JobTier::Slim`] tier (as-is);
/// re-tiering is [`retier_job`]'s job, done against the local library.
pub fn save_job(job: &Job, path: &Path) -> Result<(), OpsError> {
    let text = job.to_toml(libprintcalc::JobTier::Slim)?;
    fs::write(path, text)?;
    Ok(())
}

/// Reads and parses a job TOML file from disk, warning about any key in
/// it that no field claimed (see [`disk::warn_unknown_keys`]).
pub fn load_job(path: &Path) -> Result<Job, OpsError> {
    let text = fs::read_to_string(path)?;
    let (job, unknown) = libprintcalc::parse_toml(&text)?;
    disk::warn_unknown_keys(path, &unknown);
    Ok(job)
}

/// What a `--plate` flag asks for, including the case where it wasn't
/// given at all.
///
/// The distinction between [`PlateOverride::Keep`] and
/// [`PlateOverride::All`] is the whole reason this isn't just an
/// `Option<u32>`: a job saved with `plate = "all"` and one with no
/// `--plate` on the command line agree on the outcome, but only the
/// latter should leave the job's own stored selection alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateOverride {
    /// No `--plate` given — keep whatever the job file says.
    Keep,
    /// `--plate all` — cost every plate, whatever the job file says.
    All,
    /// `--plate N` — cost just plate `N`.
    Only(u32),
}

impl PlateOverride {
    /// Applies this override to `job`, leaving [`PlateOverride::Keep`]
    /// alone. The job in memory only — nothing is written to disk.
    pub fn apply(self, job: &mut Job) {
        match self {
            PlateOverride::Keep => {}
            PlateOverride::All => job.plate_selection = None,
            PlateOverride::Only(index) => job.plate_selection = Some(index),
        }
    }

    /// The [`Job::plate_selection`] to give a job being built from
    /// scratch, where there is no stored selection to keep.
    pub fn selection(self) -> Option<u32> {
        match self {
            PlateOverride::Keep | PlateOverride::All => None,
            PlateOverride::Only(index) => Some(index),
        }
    }
}

/// Loads the job at `job_path`, resolves its printer/filament refs (against
/// the local library, for `Linked` — embedded profiles need no lookup) and
/// its slice data (re-parsed from `source.path`, or read straight off
/// `source.summary` for a Bundled job), then runs the cost calc.
///
/// `plate` replaces the job's own `plate_selection` for this calc only —
/// the file on disk is untouched. That lets a job saved with
/// `plate = "all"` (a Bundled job whose original sliced file is long gone,
/// say) still be priced one plate at a time on demand.
///
/// `hardware` is appended to whatever the job file already lists, so
/// `--hardware` on the command line adds to a saved job rather than
/// replacing its entries. The job on disk is untouched.
pub fn calc_job(
    job_path: &Path,
    plate: PlateOverride,
    hardware: Vec<HardwareItem>,
) -> Result<CostBreakdown, OpsError> {
    let mut job = load_job(job_path)?;
    plate.apply(&mut job);
    job.hardware.extend(hardware);
    let slice = resolve_slice_summary(&job, job_path)?;
    calc_job_data(&job, &slice)
}

/// Same as [`calc_job`], but for a `job`/`slice` already in hand — used to
/// preview a job's cost without writing it to disk first (the bare
/// `printcalc <slice>` form, no `-o`). Loads the saved config for the
/// fallback defaults.
pub fn calc_job_data(job: &Job, slice: &SliceSummary) -> Result<CostBreakdown, OpsError> {
    let printer = resolve_printer(&job.printer)?;
    let material_profiles = resolve_materials(&job.material)?;
    let default_material = job
        .default_material
        .as_ref()
        .map(resolve_material)
        .transpose()?;
    let defaults = disk::load_config()?;
    Ok(cost::calculate_cost(
        job,
        slice,
        &printer,
        &material_profiles,
        default_material.as_ref(),
        &defaults,
    )?)
}

/// Re-tiers an already-saved job up to `tier` (`Full` or `Bundled` only —
/// there's no way back down to `Default` once refs/source are embedded,
/// since the original library names and file path aren't tracked once
/// they're resolved away). Resolves any `Linked` printer/filament refs
/// against the library to embed them; for `Bundled`, also parses
/// `source.path` (if it isn't already `Embedded`) to embed the
/// `SliceSummary` too — the point being the result no longer needs the
/// library or the original sliced file at calc time.
pub fn retier_job(job_path: &Path, tier: JobTier) -> Result<Job, OpsError> {
    if tier == JobTier::Default {
        return Err(OpsError::Toml(
            "can't re-tier to Default — a Full/Bundled job's linked names and original file path \
             aren't tracked anymore once embedded"
                .to_string(),
        ));
    }
    let job = load_job(job_path)?;

    let printer = ProfileRef::Embedded {
        profile: Box::new(resolve_printer(&job.printer)?),
    };
    let material = resolve_materials(&job.material)?
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
    let source = if tier == JobTier::Bundled {
        JobSource::Embedded(resolve_slice_summary(&job, job_path)?)
    } else {
        job.source
    };

    Ok(Job {
        source,
        printer,
        material,
        ..job
    })
}

fn resolve_slice_summary(job: &Job, job_path: &Path) -> Result<SliceSummary, OpsError> {
    match &job.source {
        JobSource::Embedded(summary) => Ok(summary.clone()),
        JobSource::Path(path) => {
            let resolved = if path.is_absolute() {
                path.clone()
            } else {
                job_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(path)
            };
            let bytes = fs::read(&resolved)?;
            let filename = resolved.file_name().and_then(|f| f.to_str()).unwrap_or("");
            Ok(formats::parse_slice_bytes(&bytes, filename)?)
        }
    }
}

fn resolve_printer(printer_ref: &ProfileRef<PrinterProfile>) -> Result<PrinterProfile, OpsError> {
    match printer_ref {
        ProfileRef::Embedded { profile } => Ok((**profile).clone()),
        ProfileRef::Linked { name } => show_printer(name),
    }
}

/// Turns a [`ProfileChoice`] into the [`ProfileRef`] a job of this tier
/// should store: `Default` links a name, the other tiers embed the loaded
/// profile. A `Profile` choice has no name to link, so it embeds at every
/// tier.
fn resolve_choice<T>(
    choice: ProfileChoice<T>,
    tier: JobTier,
    load: impl FnOnce(&str) -> Result<T, OpsError>,
) -> Result<ProfileRef<T>, OpsError> {
    Ok(match (choice, tier) {
        (ProfileChoice::Named(name), JobTier::Default) => ProfileRef::Linked { name },
        (ProfileChoice::Named(name), _) => ProfileRef::Embedded {
            profile: Box::new(load(&name)?),
        },
        (ProfileChoice::Profile(profile), _) => ProfileRef::Embedded { profile },
    })
}

fn resolve_material(
    material_ref: &ProfileRef<MaterialProfile>,
) -> Result<MaterialProfile, OpsError> {
    match material_ref {
        ProfileRef::Embedded { profile } => Ok((**profile).clone()),
        ProfileRef::Linked { name } => show_filament(name),
    }
}

fn resolve_materials(
    map: &HashMap<String, ProfileRef<MaterialProfile>>,
) -> Result<HashMap<String, MaterialProfile>, OpsError> {
    map.iter()
        .map(|(slot, r)| Ok((slot.clone(), resolve_material(r)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libprintcalc::formats::slice_summary::{FilamentUsage, PlateSummary, SourceFormat};
    use libprintcalc::model::filament::MaterialKind;
    use libprintcalc::model::job::SINGLE_MATERIAL_SLOT;
    use libprintcalc::model::printer::{PrinterKind, WearProfile};

    fn sl1_fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../Testfiles/Shape-Box.sl1s")
    }

    /// `Testfiles/bundled-job-example.toml` — a real Bundled-tier job
    /// (embedded profiles + embedded slice summary from `Shape-Box.sl1s`),
    /// generated via the CLI itself with `--type bundled`. Calc'ing it
    /// needs no library and no original sliced file — checked here by
    /// simply not setting `PRINTCALC_CONFIG_DIR` at all. Doesn't assert
    /// `electricity_cost`/`total_cost`: those still fall back to whatever
    /// `~/.config/printcalc/config.toml` says (electricity price isn't
    /// baked into a job even at the Bundled tier), so asserting them here
    /// would make the test depend on ambient host config instead of this
    /// fixture.
    #[test]
    fn calc_job_resolves_the_bundled_example_fixture() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../Testfiles/bundled-job-example.toml");

        // Default (no override): both plates summed.
        // Tolerances here are 1e-4: reported costs are rounded to four
        // decimal places (see `cost::COST_DP`), so they can't match an
        // exact product to more than that.
        let breakdown = calc_job(&path, PlateOverride::Keep, Vec::new()).unwrap();
        assert!((breakdown.material_cost - 0.2654538075).abs() < 1e-4);
        assert_eq!(breakdown.material_breakdown.len(), 1);
        assert_eq!(
            breakdown.material_breakdown[0].filament_name,
            "prusament-resin-orange"
        );
        assert_eq!(breakdown.labor_cost, 3.75); // $15/hr * 15 minutes
        assert_eq!(breakdown.hardware_cost, 0.64); // 4 * $0.12 + 2 * $0.08
        assert_eq!(breakdown.hardware_breakdown.len(), 2);

        // --plate 1 override: just the first plate's material.
        let plate1 = calc_job(&path, PlateOverride::Only(1), Vec::new()).unwrap();
        assert!((plate1.material_cost - 0.136426395).abs() < 1e-4);

        // --plate 3 doesn't exist (only plates 1 and 2 are in this fixture).
        assert!(matches!(
            calc_job(&path, PlateOverride::Only(3), Vec::new()).unwrap_err(),
            OpsError::Calc(CalcError::PlateNotFound(3))
        ));
    }

    #[test]
    fn new_job_parses_slice_and_builds_default_tier_job() {
        let outcome = new_job(
            NewJobParams {
                name: "test-print".into(),
                slice_path: sl1_fixture_path(),
                printer: "sl1-printer".to_string().into(),
                filaments: HashMap::from([(
                    SINGLE_MATERIAL_SLOT.to_string(),
                    "resin-grey".to_string().into(),
                )]),
                plate_selection: None,
                labor_minutes: 10.0,
                labor_rate_per_hour: None,
                default_filament: None,
                quantity: 1,
                hardware: Vec::new(),
                tier: JobTier::Default,
            },
            parse_slice(&sl1_fixture_path()).unwrap(),
        )
        .unwrap();

        assert!(!outcome.plate_selection_ignored);
        assert_eq!(outcome.job.name, "test-print");
        assert_eq!(outcome.job.plate_selection, None);
        assert_eq!(
            outcome.job.printer,
            ProfileRef::Linked {
                name: "sl1-printer".into()
            }
        );
        assert_eq!(
            outcome.job.material[SINGLE_MATERIAL_SLOT],
            ProfileRef::Linked {
                name: "resin-grey".into()
            }
        );
    }

    #[test]
    fn new_job_warns_when_plate_selection_is_moot() {
        // Shape-Box.sl1s has exactly one plate — requesting plate 2 has
        // nothing to select, so it should be reported as ignored.
        let outcome = new_job(
            NewJobParams {
                name: "test-print".into(),
                slice_path: sl1_fixture_path(),
                printer: "sl1-printer".to_string().into(),
                filaments: HashMap::from([(
                    SINGLE_MATERIAL_SLOT.to_string(),
                    "resin-grey".to_string().into(),
                )]),
                plate_selection: Some(2),
                labor_minutes: 0.0,
                labor_rate_per_hour: None,
                default_filament: None,
                quantity: 1,
                hardware: Vec::new(),
                tier: JobTier::Default,
            },
            parse_slice(&sl1_fixture_path()).unwrap(),
        )
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
                meta: Default::default(),
                plates: vec![PlateSummary {
                    plate_index: 1,
                    print_time_seconds: 3600.0,
                    filament_usage: vec![FilamentUsage {
                        slot_id: SINGLE_MATERIAL_SLOT.into(),
                        material_type: Some("Resin".into()),
                        ml: Some(10.0),
                        ..Default::default()
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
                        print_time_rate_override: None,
                    },
                }),
            },
            material: HashMap::from([(
                SINGLE_MATERIAL_SLOT.to_string(),
                ProfileRef::Embedded {
                    profile: Box::new(MaterialProfile {
                        name: "resin-grey".into(),
                        kind: MaterialKind::Resin,
                        price: 40.0,
                        efficiency_factor: 1.0,
                        color: None,
                        vendor: None,
                    }),
                },
            )]),
            default_material: None,
            quantity: 1,
            material_efficiency_factor_override: None,
            electricity_cost_per_kwh_override: None,
            labor: LaborCost {
                rate_per_hour: Some(15.0),
                minutes: 20.0,
            },
            hardware: vec![HardwareItem {
                name: "resin vat film".into(),
                cost: 2.0,
                quantity: 1,
            }],
            margin_percentages_override: None,
        };

        let path = temp_job_path("bundled");
        save_job(&job, &path).unwrap();
        let breakdown = calc_job(&path, PlateOverride::Keep, Vec::new()).unwrap();
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

        // Nothing saved yet — a built-in preset name still resolves.
        assert_eq!(show_printer("bambu-a1").unwrap().kind, PrinterKind::Fdm);
        assert!(list_printers().unwrap().contains(&"bambu-a1".to_string()));

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
                print_time_rate_override: None,
            },
        })
        .unwrap();
        add_filament(&MaterialProfile {
            name: "resin-grey".into(),
            kind: MaterialKind::Resin,
            price: 40.0,
            efficiency_factor: 1.0,
            color: None,
            vendor: None,
        })
        .unwrap();
        let printers = list_printers().unwrap();
        assert!(printers.contains(&"sl1-printer".to_string()));
        assert!(printers.contains(&"bambu-a1".to_string())); // built-in, merged in
        let filaments = list_filaments().unwrap();
        assert!(filaments.contains(&"resin-grey".to_string()));
        assert!(filaments.contains(&"pla-black".to_string())); // built-in, merged in
        assert_eq!(
            show_printer("sl1-printer").unwrap().kind,
            PrinterKind::Resin
        );

        // A saved profile shadows a built-in preset under the same name.
        add_printer(&PrinterProfile {
            name: "bambu-a1".into(),
            kind: PrinterKind::Fdm,
            wear: WearProfile {
                purchase_price: 999.0,
                ..builtin::printer("bambu-a1").unwrap().wear
            },
        })
        .unwrap();
        assert_eq!(show_printer("bambu-a1").unwrap().wear.purchase_price, 999.0);

        let outcome = new_job(
            NewJobParams {
                name: "test-print".into(),
                slice_path: sl1_fixture_path(),
                printer: "sl1-printer".to_string().into(),
                filaments: HashMap::from([(
                    SINGLE_MATERIAL_SLOT.to_string(),
                    "resin-grey".to_string().into(),
                )]),
                plate_selection: None,
                labor_minutes: 6.0,
                labor_rate_per_hour: None,
                default_filament: None,
                quantity: 1,
                hardware: Vec::new(),
                tier: JobTier::Default,
            },
            parse_slice(&sl1_fixture_path()).unwrap(),
        )
        .unwrap();

        let path = temp_job_path("default-tier");
        save_job(&outcome.job, &path).unwrap();
        let breakdown = calc_job(&path, PlateOverride::Keep, Vec::new()).unwrap();

        // Re-tiering: Default -> Bundled embeds both the profiles and the
        // parsed slice summary; the retiered job needs neither the
        // library nor the original sliced file to calc the same result.
        let bundled = retier_job(&path, JobTier::Bundled).unwrap();
        assert!(matches!(bundled.printer, ProfileRef::Embedded { .. }));
        assert!(matches!(
            bundled.material[SINGLE_MATERIAL_SLOT],
            ProfileRef::Embedded { .. }
        ));
        assert!(matches!(bundled.source, JobSource::Embedded(_)));
        let bundled_path = temp_job_path("bundled-retiered");
        save_job(&bundled, &bundled_path).unwrap();
        let _ = fs::remove_dir_all(&config_dir); // library gone...
        unsafe {
            std::env::remove_var("PRINTCALC_CONFIG_DIR");
        }
        let bundled_breakdown = calc_job(&bundled_path, PlateOverride::Keep, Vec::new()).unwrap(); // ...still calcs fine
        assert_eq!(bundled_breakdown.material_cost, breakdown.material_cost);

        assert!(matches!(
            retier_job(&path, JobTier::Default),
            Err(OpsError::Toml(_))
        ));

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&bundled_path);

        // Shape-Box.sl1s uses 2.598598 ml of resin.
        // Rounded to 4dp on the way out, so not an exact product.
        assert!((breakdown.material_cost - 2.598598 * (40.0 / 1000.0)).abs() < 1e-4);
    }
}
