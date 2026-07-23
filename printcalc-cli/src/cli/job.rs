//! `printcalc <file> [flags]` — the primary action, not a subcommand: the
//! `file` positional on the top-level `Cli` (see `main.rs`) dispatches
//! straight here. A `.toml` path calcs an existing job; anything else is
//! parsed as a sliced file and turned into one. Arg parsing + printing
//! only — all real logic is `printcalc_lib::ops::*`. See `SPEC.md` §7.
//!
//! `--filament-map` (multi-slot) lands with milestone 7; against the
//! `prusa-sl1` parser alone there's only ever one filament slot.

use std::error::Error;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::ValueEnum;

use printcalc_lib::model::cost::CostBreakdown;
use printcalc_lib::ops::{self, JobTier, NewJobParams};

use crate::interactive;

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

/// Which job tier to build — see `SPEC.md` §5. Named to match `--type`.
#[derive(Clone, Copy, ValueEnum)]
pub enum JobType {
    Default,
    Full,
    Bundled,
}

impl From<JobType> for JobTier {
    fn from(t: JobType) -> Self {
        match t {
            JobType::Default => JobTier::Default,
            JobType::Full => JobTier::Full,
            JobType::Bundled => JobTier::Bundled,
        }
    }
}

pub struct JobInvocation {
    pub file: PathBuf,
    /// Defaults to the output file's stem, or the input file's stem if no
    /// `-o`. Ignored when calc'ing an existing job (the name's already
    /// baked into the file).
    pub name: Option<String>,
    pub printer: Option<String>,
    pub filament: Option<String>,
    pub output: Option<PathBuf>,
    pub job_type: JobType,
    /// "all" (default) or a plate number; ignored with a warning for
    /// single-plate formats.
    pub plate: Option<String>,
    pub labor_minutes: Option<f64>,
    pub labor_rate: Option<f64>,
    pub format: OutputFormat,
    pub interactive: bool,
}

pub fn run(inv: JobInvocation) -> Result<(), Box<dyn Error>> {
    if inv.file.extension().and_then(|e| e.to_str()) == Some("toml") {
        let breakdown = ops::calc_job(&inv.file)?;
        print_result(&breakdown, inv.format);
        return Ok(());
    }
    new_job_flow(inv)
}

/// `file` isn't a `.toml` — parse it as a sliced file and build a job. With
/// `-o`, save it (no calc shown, matching the old `job new`). Without,
/// nothing is written at all — just resolve refs and show the cost, per
/// SPEC.md's "just pass in the [slice] file" bare-invocation form.
fn new_job_flow(inv: JobInvocation) -> Result<(), Box<dyn Error>> {
    // Fail fast on an unparseable file before asking the user anything.
    let slice = ops::parse_slice(&inv.file)?;
    let plate_selection = parse_plate(inv.plate.as_deref())?;

    let printer_name = interactive::require_choice(
        inv.printer,
        "printer",
        "Printer profile name",
        inv.interactive,
        &ops::list_printers().unwrap_or_default(),
    )?;
    let filament_name = interactive::require_choice(
        inv.filament,
        "filament",
        "Filament profile name",
        inv.interactive,
        &ops::list_filaments().unwrap_or_default(),
    )?;
    // No prompt: unlike printer/filament, omitting this just means no
    // labor cost rather than something that needs asking about.
    let labor_minutes = inv.labor_minutes.unwrap_or(0.0);
    let labor_rate = interactive::optional_f64(
        inv.labor_rate,
        "Labor rate per hour (blank = use config default)",
        inv.interactive,
    )?;

    let name = inv.name.clone().unwrap_or_else(|| {
        inv.output
            .as_ref()
            .and_then(|p| p.file_stem())
            .or_else(|| inv.file.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("job")
            .to_string()
    });

    let outcome = ops::new_job_from_slice(
        NewJobParams {
            name,
            slice_path: inv.file.clone(),
            printer_name,
            filament_name,
            plate_selection,
            labor_minutes,
            labor_rate_per_hour: labor_rate,
            tier: inv.job_type.into(),
        },
        slice,
    )?;
    if outcome.plate_selection_ignored {
        eprintln!("warning: --plate is ignored for single-plate sliced files");
    }

    match inv.output {
        Some(path) => {
            ops::save_job(&outcome.job, &path)?;
            println!("wrote {}", path.display());
        }
        None => {
            let breakdown = ops::calc_job_data(&outcome.job, &outcome.slice)?;
            print_result(&breakdown, inv.format);
        }
    }
    Ok(())
}

fn parse_plate(plate: Option<&str>) -> Result<Option<u32>, Box<dyn Error>> {
    match plate {
        None | Some("all") => Ok(None),
        Some(s) => s.parse::<u32>().map(Some).map_err(|_| format!("invalid --plate value: {s:?}").into()),
    }
}

fn print_result(breakdown: &CostBreakdown, format: OutputFormat) {
    match format {
        OutputFormat::Table => print_receipt(breakdown),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(breakdown).expect("CostBreakdown always serializes")),
    }
}

const CYAN: &str = "\x1b[36m";
const AMBER: &str = "\x1b[33m";
const ROSE: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const RESET: &str = "\x1b[0m";

/// Receipt-style, dotted-leader table format. See `SPEC.md` §7.
fn print_receipt(b: &CostBreakdown) {
    let color = std::io::stdout().is_terminal();
    let lines = [
        ("Material", b.material_cost, CYAN),
        ("Printer wear", b.wear_cost, AMBER),
        ("Electricity", b.electricity_cost, AMBER),
        ("Labor", b.labor_cost, ROSE),
        ("Hardware", b.hardware_cost, BLUE),
    ];
    let width = lines.iter().map(|(label, ..)| label.len()).max().unwrap_or(0) + 14;

    println!();
    for (label, value, code) in lines {
        let dots = ".".repeat(width.saturating_sub(label.len()));
        println!("{}", paint(&format!("  {label}{dots} ${value:.2}"), code, color));
    }
    println!("  {}", "─".repeat(width + 3));
    let dots = ".".repeat(width.saturating_sub("TOTAL".len()));
    println!(
        "{}",
        paint(&format!("  TOTAL{dots} ${:.2}", b.total_cost), BOLD_GREEN, color)
    );
    println!();

    let markups: Vec<String> = b.suggested_prices.iter().map(|p| format!("{:.0}%", p.markup_percent)).collect();
    let prices: Vec<String> = b.suggested_prices.iter().map(|p| format!("${:.2}", p.price)).collect();
    println!(
        "{}",
        paint(
            &format!("  sale price @ {}:  {}", markups.join(" / "), prices.join("  ")),
            BOLD_GREEN,
            color
        )
    );
}

fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled { format!("{code}{text}{RESET}") } else { text.to_string() }
}
