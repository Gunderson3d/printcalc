//! `printcalc` CLI. Arg parsing, interactive prompts, and terminal output
//! only — all real logic lives in `printcalc_lib::ops`.
//!
//! `printcalc <file> [flags]` is the primary action (cost a sliced file, or
//! calc an existing job `.toml`) — `printer`/`filament` are subcommands for
//! profile management. Interactive prompting is on by default for anything
//! required that wasn't passed as a flag; `--no-input` turns that off.

mod cli;
mod interactive;

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
    #[arg(short = 'm', long = "labor-minutes")]
    labor_minutes: Option<f64>,
    #[arg(short = 'r', long = "labor-rate")]
    labor_rate: Option<f64>,
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
    Printer(PrinterArgs),
    Filament(FilamentArgs),
    /// View/set global defaults (electricity cost, labor rate, markup
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
                output: cli.output,
                job_type: cli.job_type,
                retier: cli.retier,
                plate: cli.plate,
                labor_minutes: cli.labor_minutes,
                labor_rate: cli.labor_rate,
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
