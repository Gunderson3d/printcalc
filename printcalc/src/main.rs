//! `printcalc` CLI. Arg parsing, interactive prompts, terminal output,
//! and everything disk-backed — the profile library, job file I/O,
//! config persistence.
//!
//! No costing logic lives here. Parsing, matching and the arithmetic are
//! all in `libprintcalc`, which this crate and `printcalc-wasm` are both
//! thin frontends over, so the terminal and the browser cannot disagree
//! about what a job costs.
//!
//! `printcalc <file> [flags]` is the primary action (cost a sliced file, or
//! calc an existing job `.toml`) — `printer`/`filament` are subcommands for
//! profile management. Interactive prompting is on by default for anything
//! required that wasn't passed as a flag; `--no-input` turns that off.

mod cli;
mod disk;
mod interactive;
mod ops;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use cli::config::ConfigArgs;
use cli::filament::FilamentArgs;
use cli::job::{JobInvocation, JobType, OutputFormat};
use cli::printer::PrinterArgs;

#[derive(Parser)]
#[command(
    name = "printcalc",
    version,
    about = "Calculate 3D print job costs from sliced-file output."
)]
struct Cli {
    /// Sliced file to cost, or an existing job .toml to calc.
    file: Option<PathBuf>,

    /// Job name; defaults to the output file's stem, or the input file's
    /// stem if no -o.
    #[arg(short = 'n', long)]
    name: Option<String>,
    /// Printer profile to price wear and electricity against. Overrides
    /// whatever the sliced file says it was sliced for.
    #[arg(short = 'p', long)]
    printer: Option<String>,
    /// Default filament for every slot; overridden per-slot by
    /// --filament-map.
    #[arg(short = 'f', long)]
    filament: Option<String>,
    /// One filament per AMS tray/extruder: `slot=name,slot=name,...`.
    /// Slots left out fall back to --filament, if given.
    #[arg(long = "filament-map")]
    filament_map: Option<String>,
    /// Filament for any slot still unmapped at calc time — stored in the
    /// job rather than expanded per slot, so it also covers slots that
    /// only show up when the sliced file is re-parsed later.
    #[arg(long = "default-filament")]
    default_filament: Option<String>,
    /// Save the job here. Without this, nothing is written — the cost is
    /// just computed and shown.
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(short = 't', long = "type", value_enum, default_value = "default")]
    job_type: JobType,
    /// Re-tier an existing job .toml (`full` or `bundled`) instead of
    /// calc'ing it — resolves linked printer/filament refs (and, for
    /// `bundled`, the sliced file) so the result needs neither at calc
    /// time. Requires -o.
    #[arg(long = "retier", value_enum)]
    retier: Option<JobType>,
    /// "all" (default) or a plate number; ignored with a warning for
    /// single-plate formats.
    #[arg(short = 'P', long)]
    plate: Option<String>,
    /// Hands-on time **per unit** — setup, removal, support picking,
    /// finishing. Multiplied by --quantity, so charging one setup across
    /// a batch means dividing those minutes down yourself.
    #[arg(short = 'm', long = "labor-minutes")]
    labor_minutes: Option<f64>,
    /// Labor rate per hour for this job, overriding the global default
    /// set by `printcalc config --labor-rate`.
    #[arg(short = 'r', long = "labor-rate")]
    labor_rate: Option<f64>,
    /// How many units to price. Every cost and price shown is for the
    /// whole batch, with the per-unit split alongside it.
    #[arg(short = 'q', long, default_value_t = 1)]
    quantity: u32,
    /// Ignore the printer and filament the sliced file names, and ask (or
    /// fail) as if it said nothing.
    #[arg(long = "no-detect")]
    no_detect: bool,
    #[arg(short = 'F', long, value_enum, default_value = "table")]
    format: OutputFormat,
    /// Never prompt; fail fast if something required wasn't passed as a flag.
    /// No short flag: this one is global, and `-n` is already spoken for
    /// (`--name` here, `--name` on `printer`/`filament add`).
    #[arg(long = "no-input", global = true)]
    no_input: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Add, list and inspect saved printer profiles.
    Printer(PrinterArgs),
    /// Add, list and inspect saved filament and resin profiles.
    Filament(FilamentArgs),
    /// View/set global defaults (electricity cost, labor rate, margin
    /// percentages) — saved to `~/.config/printcalc/config.toml`.
    Config(ConfigArgs),
}

fn main() {
    let cli = Cli::parse();
    let interactive = !cli.no_input;

    let result = match cli.command {
        Some(Command::Printer(args)) => cli::printer::run(args.command, interactive),
        Some(Command::Filament(args)) => cli::filament::run(args.command, interactive),
        Some(Command::Config(args)) => cli::config::run(args),
        None => match cli.file {
            Some(file) => cli::job::run(JobInvocation {
                file,
                name: cli.name,
                printer: cli.printer,
                filament: cli.filament,
                filament_map: cli.filament_map,
                default_filament: cli.default_filament,
                output: cli.output,
                job_type: cli.job_type,
                retier: cli.retier,
                plate: cli.plate,
                labor_minutes: cli.labor_minutes,
                labor_rate: cli.labor_rate,
                quantity: cli.quantity,
                no_detect: cli.no_detect,
                format: cli.format,
                interactive,
            }),
            None => {
                Err("no file given — pass a sliced file or a job .toml, or run `printcalc printer`/`printcalc filament`".into())
            }
        },
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
