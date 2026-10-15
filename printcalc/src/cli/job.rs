//! The default command: cost a sliced file or a saved job `.toml`.
//!
//! This is where auto-detection meets explicit flags. A sliced file is
//! parsed, [`libprintcalc::auto_job`] guesses the printer and each
//! filament slot from what the file records, and anything the user named
//! explicitly overrides that guess. Detection then reports only what a
//! flag *didn't* already settle, so the output never suggests a choice
//! was made for the user when it wasn't.
//!
//! Detection notes and warnings go to stderr, keeping stdout clean for
//! `--format json`.

use std::collections::HashMap;
use std::error::Error;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::ValueEnum;

use libprintcalc::formats::slice_summary::{SliceSummary, SlotUsage};
use libprintcalc::model::cost::CostBreakdown;
use libprintcalc::model::filament::MaterialProfile;
use libprintcalc::model::job::HardwareItem;
use libprintcalc::model::printer::PrinterProfile;

use crate::interactive;
use crate::ops::{self, JobTier, NewJobParams, ProfileChoice};

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

/// Which job tier to build. Named to match `--type`.
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
    /// Default filament for every slot; `filament_map` overrides specific
    /// slots on top of it.
    pub filament: Option<String>,
    /// `slot=name,slot=name,...` — one filament per AMS tray / extruder.
    /// Slots it doesn't cover fall back to `filament`, if given.
    pub filament_map: Option<String>,
    /// Stored on the job as its fallback for any slot still unmapped at
    /// calc time, rather than expanded per slot like `filament`.
    pub default_filament: Option<String>,
    pub output: Option<PathBuf>,
    pub job_type: JobType,
    /// "all" (default) or a plate number; ignored with a warning for
    /// single-plate formats.
    pub plate: Option<String>,
    pub labor_minutes: Option<f64>,
    pub labor_rate: Option<f64>,
    /// How many units to price. All costs and prices are for the batch.
    pub quantity: u32,
    /// `--hardware` entries, unparsed: `name:unit price[:units]`.
    pub hardware: Vec<String>,
    /// Skip reading the printer/filament the sliced file names, and ask
    /// (or fail) as if it said nothing.
    pub no_detect: bool,
    pub format: OutputFormat,
    /// `false` for `--no-color`. Even when `true`, color only shows on a
    /// terminal with `NO_COLOR` unset.
    pub color: bool,
    pub interactive: bool,
    /// Re-tier an existing job (`full` or `bundled` only) instead of
    /// calc'ing it — only meaningful when `file` is a `.toml`. Needs `-o`.
    pub retier: Option<JobType>,
}

pub fn run(inv: JobInvocation) -> Result<(), Box<dyn Error>> {
    if inv.file.extension().and_then(|e| e.to_str()) == Some("toml") {
        if let Some(job_type) = inv.retier {
            let output = inv
                .output
                .ok_or("--retier needs -o/--output to save the re-tiered job to")?;
            let retiered = ops::retier_job(&inv.file, job_type.into())?;
            ops::save_job(&retiered, &output)?;
            println!("wrote {}", output.display());
            return Ok(());
        }
        // --plate here overrides the job's own stored plate_selection —
        // persisted if -o saves a copy below, calc-only otherwise (e.g.
        // pricing just one plate out of a Bundled job saved with "all",
        // without needing the original sliced file).
        let plate = parse_plate(inv.plate.as_deref())?;
        let hardware = parse_hardware(&inv.hardware)?;
        if let Some(output) = &inv.output {
            let mut job = ops::load_job(&inv.file)?;
            plate.apply(&mut job);
            job.hardware.extend(hardware);
            ops::save_job(&job, output)?;
            println!("wrote {}", output.display());
            return Ok(());
        }
        let breakdown = ops::calc_job(&inv.file, plate, hardware)?;
        print_result(&breakdown, inv.format, inv.color);
        return Ok(());
    }
    new_job_flow(inv)
}

/// `file` isn't a `.toml` — parse it as a sliced file and build a job. With
/// `-o`, save it (no calc shown, matching the old `job new`). Without,
/// nothing is written at all — just resolve refs and show the cost, for
/// the bare `printcalc <slice>` invocation form.
fn new_job_flow(inv: JobInvocation) -> Result<(), Box<dyn Error>> {
    let slice = ops::parse_slice(&inv.file)?;
    let plate_selection = parse_plate(inv.plate.as_deref())?.selection();
    let hardware = parse_hardware(&inv.hardware)?;

    // Parsed up front so detection can tell which slots a flag already
    // covers and stay quiet about those.
    let mapped: HashMap<String, ProfileChoice<MaterialProfile>> = match &inv.filament_map {
        Some(s) => parse_filament_map(s)?,
        None => HashMap::new(),
    };

    // What the sliced file itself says about the setup. Explicit flags
    // still win; this only fills in what wasn't passed, and only reports
    // on what it might actually be used for.
    let detected = (!inv.no_detect).then(|| ops::detect_profiles(&slice));
    if let Some(detected) = &detected {
        let covered = |slot: &str| inv.filament.is_some() || mapped.contains_key(slot);
        print_detected(&inv.file, &slice, detected, inv.printer.is_none(), &covered);
    }

    let printer = resolve_printer_choice(inv.printer, detected.as_ref(), inv.interactive)?;
    let filaments = resolve_filaments(
        &slice,
        detected.as_ref(),
        inv.filament,
        mapped,
        inv.default_filament.is_some(),
        inv.interactive,
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

    let outcome = ops::new_job(
        NewJobParams {
            name,
            slice_path: inv.file.clone(),
            printer,
            filaments,
            default_filament: inv.default_filament.map(ProfileChoice::Named),
            quantity: inv.quantity,
            hardware,
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
            print_result(&breakdown, inv.format, inv.color);
        }
    }
    Ok(())
}

/// Everything the sliced file identified, on stderr so `--format json`
/// keeps stdout clean. Nothing here is authoritative — it's shown so a
/// wrong guess is visible before it silently prices the job.
///
/// Only what might actually get used is reported: `want_printer` and
/// `want_slot` are false for anything an explicit flag already covers,
/// since printing a guess that's about to be overridden is just
/// misleading.
fn print_detected(
    path: &std::path::Path,
    slice: &SliceSummary,
    detected: &ops::Detected,
    want_printer: bool,
    want_slot: &dyn Fn(&str) -> bool,
) {
    let mut lines: Vec<String> = Vec::new();

    if let Some(slicer) = &slice.meta.slicer {
        lines.push(format!("  slicer:   {slicer}"));
    }
    if want_printer {
        let model = slice
            .meta
            .printer_profile
            .as_deref()
            .or_else(|| slice.printer_model());
        match (&detected.printer, model) {
            (Some(m), _) => lines.push(format!(
                "  printer:  {} -> {} ({})",
                m.matched_on,
                m.profile.name,
                confidence_label(m.confidence)
            )),
            (None, Some(model)) => {
                lines.push(format!("  printer:  {model} -> no matching profile"))
            }
            (None, None) => {}
        }
    }
    for slot in &detected.slots {
        if want_slot(&slot.usage.slot_id) {
            continue;
        }
        let described = slot.usage.label();
        lines.push(match (&slot.matched, &slot.from_file) {
            (Some(m), _) => format!(
                "  {described} -> {} ({})",
                m.profile.name,
                confidence_label(m.confidence)
            ),
            (None, Some(profile)) => format!(
                "  {described} -> priced from the file at ${:.2}/{}",
                profile.price,
                if slot.usage.ml > 0.0 { "L" } else { "kg" }
            ),
            (None, None) => format!("  {described} -> no matching profile"),
        });
    }

    if lines.is_empty() {
        return;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sliced file");
    eprintln!("detected from {name}");
    for line in lines {
        eprintln!("{line}");
    }
}

/// "exact" is reserved for a name that matched outright; everything else
/// says "guess" so a near-miss is never mistaken for a certainty.
fn confidence_label(confidence: f64) -> &'static str {
    if confidence >= 0.999 {
        "exact"
    } else {
        "guess"
    }
}

/// `--printer` if given, else the detected match, else a prompt (or, under
/// `--no-input`, an error naming the flag).
fn resolve_printer_choice(
    flag: Option<String>,
    detected: Option<&ops::Detected>,
    interactive_mode: bool,
) -> Result<ProfileChoice<PrinterProfile>, Box<dyn Error>> {
    if let Some(name) = flag {
        return Ok(ProfileChoice::Named(name));
    }
    if let Some(matched) = detected.and_then(|d| d.printer.as_ref()) {
        return Ok(ProfileChoice::Named(matched.profile.name.clone()));
    }
    let name = interactive::require_choice(
        None,
        "printer",
        "Printer profile name",
        interactive_mode,
        &ops::list_printers().unwrap_or_default(),
    )?;
    Ok(ProfileChoice::Named(name))
}

/// Builds the slot_id -> filament map for a new job, most specific source
/// first: `map` (already parsed from `--filament-map`), then `--filament`
/// as a default for the rest, then whatever the sliced file identified,
/// then a prompt per remaining slot (or, under `--no-input`, an error
/// naming what's still unpriced).
///
/// `has_default` is `--default-filament`: it's stored on the job rather
/// than expanded here, but it does mean an unmapped slot isn't a problem
/// worth asking about.
fn resolve_filaments(
    slice: &SliceSummary,
    detected: Option<&ops::Detected>,
    filament: Option<String>,
    mut map: HashMap<String, ProfileChoice<MaterialProfile>>,
    has_default: bool,
    interactive_mode: bool,
) -> Result<HashMap<String, ProfileChoice<MaterialProfile>>, Box<dyn Error>> {
    let slots = slice.slots();

    // A slice with no usage at all still needs somewhere to put the one
    // filament the user names.
    let slot_ids: Vec<String> = if slots.is_empty() {
        vec!["1".to_string()]
    } else {
        slots.iter().map(|s| s.slot_id.clone()).collect()
    };

    if let Some(default_name) = &filament {
        for slot in &slot_ids {
            map.entry(slot.clone())
                .or_insert_with(|| ProfileChoice::Named(default_name.clone()));
        }
    }

    for slot in detected.map(|d| d.slots.as_slice()).unwrap_or_default() {
        if let Some(profile) = slot.profile() {
            map.entry(slot.usage.slot_id.clone()).or_insert_with(|| {
                // A matched library/built-in profile keeps its name so the
                // job can link it; one built from the file's own numbers
                // has no name to link and travels by value.
                match &slot.matched {
                    Some(m) => ProfileChoice::Named(m.profile.name.clone()),
                    None => ProfileChoice::Profile(Box::new(profile.clone())),
                }
            });
        }
    }

    let missing: Vec<&SlotUsage> = slots
        .iter()
        .filter(|slot| !map.contains_key(&slot.slot_id))
        .collect();
    if missing.is_empty() || has_default {
        // Nothing named and nothing detected: the single-slot case still
        // deserves the plain, un-slot-labeled prompt.
        if map.is_empty() && !has_default {
            let name = interactive::require_choice(
                None,
                "filament",
                "Filament profile name",
                interactive_mode,
                &ops::list_filaments().unwrap_or_default(),
            )?;
            return Ok(HashMap::from([(
                slot_ids[0].clone(),
                ProfileChoice::Named(name),
            )]));
        }
        return Ok(map);
    }

    if !interactive_mode {
        let listed: Vec<String> = missing.iter().map(|s| s.label()).collect();
        return Err(format!(
            "no filament for {}; pass --filament as a default, or --filament-map slot=name,... per slot",
            listed.join(", ")
        )
        .into());
    }

    let choices = ops::list_filaments().unwrap_or_default();
    for slot in missing {
        // One slot and nothing else in play reads better without the
        // slot label, same as it always has.
        let label = if slots.len() == 1 {
            "Filament profile name".to_string()
        } else {
            format!("Filament profile for {}", slot.label())
        };
        let name = interactive::require_choice(None, "filament", &label, true, &choices)?;
        map.insert(slot.slot_id.clone(), ProfileChoice::Named(name));
    }
    Ok(map)
}

/// `--plate`: absent, `"all"`, or a plate number.
fn parse_plate(plate: Option<&str>) -> Result<ops::PlateOverride, Box<dyn Error>> {
    match plate {
        None => Ok(ops::PlateOverride::Keep),
        Some("all") => Ok(ops::PlateOverride::All),
        Some(s) => s
            .parse::<u32>()
            .map(ops::PlateOverride::Only)
            .map_err(|_| format!("invalid --plate value: {s:?}").into()),
    }
}

fn parse_filament_map(
    s: &str,
) -> Result<HashMap<String, ProfileChoice<MaterialProfile>>, Box<dyn Error>> {
    s.split(',')
        .map(|pair| {
            let (slot, name) = pair.split_once('=').ok_or_else(|| {
                format!("invalid --filament-map entry {pair:?}, expected slot=name")
            })?;
            Ok((
                slot.trim().to_string(),
                ProfileChoice::Named(name.trim().to_string()),
            ))
        })
        .collect()
}

/// `--hardware` entries: `name:unit price[:units]`, one [`HardwareItem`]
/// each. Split from the right so a name may itself contain colons
/// (`M3x5:brass:0.08:4` is a name of `M3x5:brass`). Units default to 1,
/// matching the job file's default.
fn parse_hardware(entries: &[String]) -> Result<Vec<HardwareItem>, Box<dyn Error>> {
    entries.iter().map(|e| parse_hardware_entry(e)).collect()
}

fn parse_hardware_entry(entry: &str) -> Result<HardwareItem, Box<dyn Error>> {
    let invalid = || -> Box<dyn Error> {
        format!("invalid --hardware entry {entry:?}, expected name:unit-price[:units]").into()
    };
    // rsplitn yields right-to-left: [units?, price, name].
    let mut parts: Vec<&str> = entry.rsplitn(3, ':').map(str::trim).collect();
    parts.reverse();
    let (name, price, units) = match parts.as_slice() {
        [name, price] => (*name, *price, "1"),
        [name, price, units] => (*name, *price, *units),
        _ => return Err(invalid()),
    };
    if name.is_empty() {
        return Err(invalid());
    }
    let cost: f64 = price
        .trim_start_matches('$')
        .parse()
        .map_err(|_| format!("invalid --hardware unit price {price:?} in {entry:?}"))?;
    if !cost.is_finite() || cost < 0.0 {
        return Err(format!("invalid --hardware unit price {price:?} in {entry:?}").into());
    }
    let quantity: u32 = units.parse().ok().filter(|q| *q > 0).ok_or_else(|| {
        format!("invalid --hardware units {units:?} in {entry:?} (whole number, at least 1)")
    })?;
    Ok(HardwareItem {
        name: name.to_string(),
        cost,
        quantity,
    })
}

fn print_result(breakdown: &CostBreakdown, format: OutputFormat, color: bool) {
    match format {
        OutputFormat::Table => print_receipt(breakdown, color && color_allowed()),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(breakdown).expect("CostBreakdown always serializes")
        ),
    }
}

/// Color is for eyes, not pipes: off when stdout isn't a terminal, and
/// off when `NO_COLOR` is set to anything (https://no-color.org).
fn color_allowed() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

const CYAN: &str = "\x1b[36m";
const AMBER: &str = "\x1b[33m";
const ROSE: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const RESET: &str = "\x1b[0m";

/// The per-plate table, shown only for a multi-plate file — a single
/// plate would just restate the summary.
fn print_plate_section(b: &CostBreakdown, width: usize) {
    if b.plate_breakdown.len() <= 1 {
        return;
    }
    println!("plates");
    for p in &b.plate_breakdown {
        let label = format!("Plate {}", p.plate_index);
        let detail = format!(
            "{} · {}",
            format_duration(p.print_time_seconds),
            format_amount(p.grams, p.ml)
        );
        print_dotted(&label, &detail, width);
    }
    let combined_seconds: f64 = b.plate_breakdown.iter().map(|p| p.print_time_seconds).sum();
    let combined_grams: f64 = b.plate_breakdown.iter().map(|p| p.grams).sum();
    let combined_ml: f64 = b.plate_breakdown.iter().map(|p| p.ml).sum();
    println!("  {}", "─".repeat(width + 3));
    let detail = format!(
        "{} · {}",
        format_duration(combined_seconds),
        format_amount(combined_grams, combined_ml)
    );
    print_dotted("Combined", &detail, width);
    println!();
}

/// One line per contributing filament profile. Slots sharing a profile
/// are already collapsed into one line by the calculation.
fn print_material_section(b: &CostBreakdown, width: usize, color: bool) {
    if b.material_breakdown.is_empty() {
        return;
    }
    println!("material breakdown");
    for line in &b.material_breakdown {
        let price_unit = if line.unit == "g" { "kg" } else { "L" };
        let detail = format!(
            "{:.2}{} @ ${:.2}/{}",
            line.amount, line.unit, line.unit_price, price_unit
        );
        print_item_line(&line.filament_name, &detail, line.cost, width, CYAN, color);
    }
    println!();
}

/// One line per hardware entry, never aggregated — each is already its
/// own line item. For a batch the per-unit count and the multiplier are
/// both shown, since `quantity` scaling is easy to forget.
fn print_hardware_section(b: &CostBreakdown, width: usize, color: bool) {
    if b.hardware_breakdown.is_empty() {
        return;
    }
    println!("hardware breakdown");
    for line in &b.hardware_breakdown {
        let each = if b.quantity > 1 {
            format!(
                "x{} @ ${:.2} × {}",
                line.quantity, line.unit_cost, b.quantity
            )
        } else {
            format!("x{} @ ${:.2}", line.quantity, line.unit_cost)
        };
        print_item_line(&line.name, &each, line.cost, width, BLUE, color);
    }
    println!();
}

/// Receipt-style, dotted-leader table format, split into sections.
///
/// "plates" only shows when more than one plate is actually in play — a
/// single selected plate (whether because the job only has one, or
/// because `--plate N` picked one out) is the "omit entirely" case, same
/// rule `material breakdown`/`hardware breakdown` follow. The per-unit
/// column follows the same rule against `quantity`.
///
/// Every money figure is for the whole batch; the trailing summary line
/// spells out the per-unit split when there's more than one unit.
fn print_receipt(b: &CostBreakdown, color: bool) {
    let batch = b.quantity > 1;

    let cost_labels = [
        "Material",
        "Printer wear",
        "Electricity",
        "Labor",
        "Hardware",
        "TOTAL",
    ];
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
    print_plate_section(b, width);
    print_material_section(b, width, color);
    print_hardware_section(b, width, color);

    if batch {
        println!("summary  ({} units)", b.quantity);
    } else {
        println!("summary");
    }
    print_cost_line("Material", b.material_cost, width, CYAN, color);
    print_cost_line("Printer wear", b.wear_cost, width, AMBER, color);
    print_cost_line("Electricity", b.electricity_cost, width, AMBER, color);
    print_cost_line("Labor", b.labor_cost, width, ROSE, color);
    print_cost_line("Hardware", b.hardware_cost, width, BLUE, color);
    println!("  {}", "─".repeat(width + 3));
    print_cost_line("TOTAL", b.total_cost, width, BOLD_GREEN, color);
    if batch {
        print_cost_line("  per unit", b.unit_cost, width, BOLD_GREEN, color);
    }
    println!();

    // Margins, not markups: price = cost / (1 - margin). The header says
    // so, because the difference between the two is large and silent.
    let margins: Vec<String> = b
        .suggested_prices
        .iter()
        .map(|p| format!("{:.0}%", p.margin_percent))
        .collect();
    let prices: Vec<String> = b
        .suggested_prices
        .iter()
        .map(|p| format!("${:.2}", p.price))
        .collect();
    let header = format!("  sale price @ {} margin:", margins.join(" / "));
    println!(
        "{}",
        paint(
            &format!("{header}  {}", prices.join("  ")),
            BOLD_GREEN,
            color
        )
    );
    if batch {
        let per_unit: Vec<String> = b
            .suggested_prices
            .iter()
            .map(|p| format!("${:.2}", p.unit_price))
            .collect();
        // Padded to the header so the two rows of prices line up.
        let label = format!("{:<width$}", "  per unit:", width = header.len());
        println!(
            "{}",
            paint(
                &format!("{label}  {}", per_unit.join("  ")),
                BOLD_GREEN,
                color
            )
        );
    }
}

/// `label` + dotted leader (to `width`) + `trailing`, uncolored — used for
/// the plates section, which isn't part of the cost color scheme.
fn print_dotted(label: &str, trailing: &str, width: usize) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!("  {label}{dots} {trailing}");
}

fn print_cost_line(label: &str, value: f64, width: usize, code: &str, color: bool) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!(
        "{}",
        paint(&format!("  {label}{dots} ${value:.2}"), code, color)
    );
}

/// A breakdown row: `label` + dotted leader + `detail` (the "amount @
/// price" part) + ` = $cost`.
fn print_item_line(label: &str, detail: &str, cost: f64, width: usize, code: &str, color: bool) {
    let dots = ".".repeat(width.saturating_sub(label.len()));
    println!(
        "{}",
        paint(
            &format!("  {label}{dots} {detail} = ${cost:.2}"),
            code,
            color
        )
    );
}

/// "17m17s" / "1h05m17s" style.
fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}h{m:02}m{s:02}s")
    } else {
        format!("{m}m{s:02}s")
    }
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
    if enabled {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_price_units() {
        let item = parse_hardware_entry("M3 brass insert:0.08:4").unwrap();
        assert_eq!(item.name, "M3 brass insert");
        assert_eq!(item.cost, 0.08);
        assert_eq!(item.quantity, 4);
    }

    #[test]
    fn units_default_to_one_and_dollar_sign_is_tolerated() {
        let item = parse_hardware_entry("box:$0.60").unwrap();
        assert_eq!(item.name, "box");
        assert_eq!(item.cost, 0.60);
        assert_eq!(item.quantity, 1);
    }

    #[test]
    fn name_may_contain_colons() {
        let item = parse_hardware_entry("magnet 6:3:0.12:2").unwrap();
        assert_eq!(item.name, "magnet 6:3");
        assert_eq!(item.cost, 0.12);
        assert_eq!(item.quantity, 2);
    }

    #[test]
    fn rejects_malformed_entries() {
        for bad in [
            "box",
            ":0.5",
            "box:abc",
            "box:-1",
            "box:0.5:0",
            "box:0.5:1.5",
            "box:0.5:x",
        ] {
            assert!(
                parse_hardware_entry(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn parses_many() {
        let items = parse_hardware(&["a:1".into(), "b:2:3".into()]).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].quantity, 3);
    }
}
