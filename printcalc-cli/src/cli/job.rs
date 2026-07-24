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
        // --plate here overrides the job's own stored plate_selection for
        // this calc only — e.g. pricing just one plate out of a Bundled
        // job saved with "all", without needing the original sliced file.
        let plate_override = parse_plate_override(inv.plate.as_deref())?;
        let breakdown = ops::calc_job(&inv.file, plate_override)?;
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

/// Same values as [`parse_plate`], but distinguishes "not given" (outer
/// `None`, don't touch the job's own `plate_selection`) from "given as
/// `all`" (`Some(None)`, explicitly override to every plate).
fn parse_plate_override(plate: Option<&str>) -> Result<Option<Option<u32>>, Box<dyn Error>> {
    match plate {
        None => Ok(None),
        Some(_) => parse_plate(plate).map(Some),
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

/// Receipt-style, dotted-leader table format, split into sections. See
/// `SPEC.md` §7.
///
/// "plates" only shows when more than one plate is actually in play — a
/// single selected plate (whether because the job only has one, or
/// because `--plate N` picked one out) is the "omit entirely" case from
/// SPEC.md §7, same rule `material breakdown`/`hardware breakdown` follow.
fn print_receipt(b: &CostBreakdown) {
    let color = std::io::stdout().is_terminal();

    let cost_labels = ["Material", "Printer wear", "Electricity", "Labor", "Hardware", "TOTAL"];
    let plate_labels = ["Combined".to_string()];
    let width = b
        .plate_breakdown
        .iter()
        .map(|p| format!("Plate {}", p.plate_index).len())
        .chain(plate_labels.iter().map(|s| s.len()))
        .chain(b.material_breakdown.iter().map(|l| l.filament_name.len()))
        .chain(b.hardware_breakdown.iter().map(|l| l.name.len()))
        .chain(cost_labels.iter().map(|s| s.len()))
        .max()
        .unwrap_or(0)
        + 14;

    println!();
    if b.plate_breakdown.len() > 1 {
        println!("plates");
        for p in &b.plate_breakdown {
            let label = format!("Plate {}", p.plate_index);
            let detail = format!("{} · {}", format_duration(p.print_time_seconds), format_amount(p.grams, p.ml));
            print_dotted(&label, &detail, width);
        }
        let combined_seconds: f64 = b.plate_breakdown.iter().map(|p| p.print_time_seconds).sum();
        let combined_grams: f64 = b.plate_breakdown.iter().map(|p| p.grams).sum();
        let combined_ml: f64 = b.plate_breakdown.iter().map(|p| p.ml).sum();
        println!("  {}", "─".repeat(width + 3));
        let detail = format!("{} · {}", format_duration(combined_seconds), format_amount(combined_grams, combined_ml));
        print_dotted("Combined", &detail, width);
        println!();
    }

    if !b.material_breakdown.is_empty() {
        println!("material breakdown");
        for line in &b.material_breakdown {
            let price_unit = if line.unit == "g" { "kg" } else { "L" };
            let detail = format!("{:.2}{} @ ${:.2}/{}", line.amount, line.unit, line.unit_price, price_unit);
            print_item_line(&line.filament_name, &detail, line.cost, width, CYAN, color);
        }
        println!();
    }

    if !b.hardware_breakdown.is_empty() {
        println!("hardware breakdown");
        for line in &b.hardware_breakdown {
            let detail = format!("x{} @ ${:.2}", line.quantity, line.unit_cost);
            print_item_line(&line.name, &detail, line.cost, width, BLUE, color);
        }
        println!();
    }

    println!("summary");
    print_cost_line("Material", b.material_cost, width, CYAN, color);
    print_cost_line("Printer wear", b.wear_cost, width, AMBER, color);
    print_cost_line("Electricity", b.electricity_cost, width, AMBER, color);
    print_cost_line("Labor", b.labor_cost, width, ROSE, color);
    print_cost_line("Hardware", b.hardware_cost, width, BLUE, color);
    println!("  {}", "─".repeat(width + 3));
    print_cost_line("TOTAL", b.total_cost, width, BOLD_GREEN, color);
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

/// `label` + dotted leader (to `width`) + `trailing`, uncolored — used for
/// the plates section, which isn't part of SPEC.md §7's cost color scheme.
fn print_dotted(label: &str, trailing: &str, width: usize) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!("  {label}{dots} {trailing}");
}

fn print_cost_line(label: &str, value: f64, width: usize, code: &str, color: bool) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!("{}", paint(&format!("  {label}{dots} ${value:.2}"), code, color));
}

/// A breakdown row: `label` + dotted leader + `detail` (the "amount @
/// price" part) + ` = $cost`.
fn print_item_line(label: &str, detail: &str, cost: f64, width: usize, code: &str, color: bool) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!("{}", paint(&format!("  {label}{dots} {detail} = ${cost:.2}"), code, color));
}

/// "17m17s" / "1h05m17s" style, matching SPEC.md §7's plate mockup.
fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 { format!("{h}h{m:02}m{s:02}s") } else { format!("{m}m{s:02}s") }
}

/// "6.66g", "2.60ml", or "6.66g + 2.60ml" if a plate somehow has both
/// (grams/ml are 0.0 rather than absent when not applicable).
fn format_amount(grams: f64, ml: f64) -> String {
    match (grams > 0.0, ml > 0.0) {
        (true, true) => format!("{grams:.2}g + {ml:.2}ml"),
        (true, false) => format!("{grams:.2}g"),
        (false, true) => format!("{ml:.2}ml"),
        (false, false) => "0g".to_string(),
    }
}

fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled { format!("{code}{text}{RESET}") } else { text.to_string() }
}
