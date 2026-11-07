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
//! calc an existing job `.toml`; with no file, `--print-time` and
//! `--grams`/`--ml` cost a print entered by hand) — `printer`/`filament` are subcommands for
//! profile management. Interactive prompting is on by default for anything
//! required that wasn't passed as a flag; `--no-input` turns that off.

mod cli;
mod disk;
mod interactive;
mod ops;

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use cli::config::ConfigArgs;
use cli::filament::FilamentArgs;
use cli::job::{JobInput, JobInvocation, JobType, ManualPrint, OutputFormat};
use cli::printer::PrinterArgs;

#[derive(Parser)]
#[command(
    name = "printcalc",
    version,
    about = "Calculate 3D print job costs from sliced-file output."
)]
struct Cli {
    /// Sliced file to cost, or an existing job .toml to calc. Leave it out
    /// to enter the print by hand with --print-time and --grams or --ml.
    file: Option<PathBuf>,

    /// No sliced file: how long the print takes, e.g. `1h30m`, `45m`,
    /// `1.5h` or `1:30`. Use with --grams or --ml.
    #[arg(long = "print-time", value_name = "DURATION", conflicts_with = "file")]
    print_time: Option<String>,
    /// No sliced file: filament used, in grams. One number for a single
    /// material, or `slot=grams,...` per slot (`1=50,2=12.5`).
    #[arg(
        long,
        value_name = "AMOUNTS",
        conflicts_with_all = ["file", "ml"]
    )]
    grams: Option<String>,
    /// No sliced file: resin used, in ml. One number, or `slot=ml,...`
    /// per slot. Needs a resin printer and resin profiles.
    #[arg(long, value_name = "AMOUNTS", conflicts_with = "file")]
    ml: Option<String>,

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
    /// Save the job here as well as showing its cost. Without this,
    /// nothing is written.
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
    /// whole batch, with the per-unit split alongside it. On a job .toml,
    /// overrides the saved quantity. [default: 1]
    #[arg(short = 'q', long)]
    quantity: Option<u32>,
    /// Electricity cost per kWh for this job, overriding the global
    /// default set by `printcalc config --electricity-cost`.
    #[arg(short = 'e', long = "electricity-cost")]
    electricity_cost: Option<f64>,
    /// Gross margin percentages to suggest prices at for this job,
    /// comma-separated (e.g. `40,55`), overriding `printcalc config
    /// --margin`. Margins, not markups: 60 prices at cost / 0.40.
    #[arg(long = "margin", alias = "markup", value_name = "LIST")]
    margin: Option<String>,
    /// Waste allowance for this job, replacing every material's own
    /// efficiency factor (1.2 prices 20% more material than sliced).
    #[arg(long = "efficiency-factor")]
    efficiency_factor: Option<f64>,
    /// Extra per-unit material or hardware cost — inserts, magnets,
    /// packaging — as `<name>:<unit price>:<number of units>`. Units are
    /// per print and default to 1; --quantity scales them. Repeatable.
    /// On a job .toml, replaces the saved entry with the same name, or
    /// adds a new one.
    #[arg(
        short = 'H',
        long = "hardware",
        visible_alias = "extra",
        value_name = "NAME:PRICE[:UNITS]"
    )]
    hardware: Vec<String>,
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
    /// Plain output, no ANSI colors. Color is already off when stdout
    /// isn't a terminal or the NO_COLOR environment variable is set.
    #[arg(long = "no-color", global = true)]
    no_color: bool,

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
    /// percentages, currency symbol) — saved to
    /// `~/.config/printcalc/config.toml`.
    Config(ConfigArgs),
    /// Print a shell completion script to stdout. E.g. `printcalc
    /// completions zsh > ~/.zfunc/_printcalc`.
    Completions {
        /// bash, elvish, fish, powershell or zsh.
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Print the man page (roff) to stdout. E.g. `printcalc man >
    /// ~/.local/share/man/man1/printcalc.1`.
    Man,
}

fn main() {
    let cli = Cli::parse();
    let interactive = !cli.no_input;

    let result = match cli.command {
        Some(Command::Printer(args)) => cli::printer::run(args.command, interactive),
        Some(Command::Filament(args)) => cli::filament::run(args.command, interactive),
        Some(Command::Config(args)) => cli::config::run(args),
        Some(Command::Completions { shell }) => {
            // Into a buffer first: `generate` panics on a write error, and
            // `printcalc completions zsh | head` closing the pipe early
            // shouldn't look like a crash.
            let mut out = Vec::new();
            clap_complete::generate(shell, &mut Cli::command(), "printcalc", &mut out);
            std::io::Write::write_all(&mut std::io::stdout(), &out).map_err(|e| e.into())
        }
        Some(Command::Man) => {
            let mut out = Vec::new();
            clap_mangen::Man::new(Cli::command())
                .render(&mut out)
                .and_then(|()| std::io::Write::write_all(&mut std::io::stdout(), &out))
                .map_err(|e| e.into())
        }
        None => job_input(cli.file, cli.print_time, cli.grams, cli.ml).and_then(|input| {
            cli::job::run(JobInvocation {
                input,
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
                electricity_cost: cli.electricity_cost,
                margin: cli.margin,
                efficiency_factor: cli.efficiency_factor,
                hardware: cli.hardware,
                no_detect: cli.no_detect,
                format: cli.format,
                color: !cli.no_color,
                interactive,
            })
        }),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// The file to cost (made absolute, so a saved job's path doesn't depend
/// on where it was run from), or the hand-entered print when there's no
/// file but one of its flags was given.
fn job_input(
    file: Option<PathBuf>,
    print_time: Option<String>,
    grams: Option<String>,
    ml: Option<String>,
) -> Result<JobInput, Box<dyn Error>> {
    if let Some(file) = file {
        let absolute = fs::canonicalize(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        return Ok(JobInput::File(absolute));
    }
    if print_time.is_none() && grams.is_none() && ml.is_none() {
        return Err("no file given — pass a sliced file or a job .toml, enter a print by hand with --print-time and --grams or --ml, or run `printcalc printer`/`printcalc filament`".into());
    }
    Ok(JobInput::Manual(ManualPrint {
        print_time,
        grams,
        ml,
    }))
}
