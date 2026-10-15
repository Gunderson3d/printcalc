//! `printcalc printer` — add, edit, remove, list and inspect saved
//! printer profiles.
//!
//! `add` requires only a name: every numeric field falls back to
//! [`WearProfile::default`], prompting with that default shown when
//! interactive and taking it silently under `--no-input`. `edit` works
//! the same way with the saved values as the defaults, so a flag changes
//! one field and leaves the rest alone.

use std::error::Error;

use clap::{Args, Subcommand};

use libprintcalc::model::printer::{PrinterKind, PrinterProfile, WearProfile};

use crate::interactive;
use crate::ops;

#[derive(Args)]
pub struct PrinterArgs {
    #[command(subcommand)]
    pub command: PrinterCommand,
}

/// The settable fields, shared by `add` and `edit`. Every one is
/// optional at the flag level; what an omitted one means depends on the
/// command (the documented default for `add`, the saved value for
/// `edit`).
#[derive(Args)]
pub struct PrinterFields {
    /// "fdm" or "resin". [default: fdm]
    #[arg(short, long)]
    kind: Option<String>,
    /// What the machine itself cost. [default: 800]
    #[arg(long = "purchase-price")]
    purchase_price: Option<f64>,
    /// Anything else spent getting it running — enclosure, AMS,
    /// upgrades, tooling. [default: 0]
    #[arg(long = "additional-upfront-cost")]
    additional_upfront_cost: Option<f64>,
    /// Depreciation horizon: how long before you replace it.
    /// [default: 5]
    #[arg(long = "estimated-life-years")]
    estimated_life_years: Option<f64>,
    /// Fraction of wall-clock time the printer is actually printing,
    /// 0.0-1.0. Lower means each hour carries more depreciation.
    /// [default: 0.5]
    #[arg(long = "estimated-uptime-percent")]
    estimated_uptime_percent: Option<f64>,
    /// Yearly spend on consumable parts — nozzles, belts, plates.
    /// [default: 80]
    #[arg(long = "annual-maintenance-cost")]
    annual_maintenance_cost: Option<f64>,
    /// Average draw while printing, in watts. Priced at the
    /// electricity rate from `printcalc config`. [default: 160]
    #[arg(long = "power-consumption-watts")]
    power_consumption_watts: Option<f64>,
    /// Safety margin applied to the computed hourly rate, over the
    /// raw depreciation math. [default: 1.3]
    #[arg(long = "cost-buffer-factor")]
    cost_buffer_factor: Option<f64>,
    /// Flat cost per printing hour, replacing the whole computed
    /// machine rate (depreciation, electricity and buffer factor).
    #[arg(long = "print-time-rate", conflicts_with = "no_print_time_rate")]
    print_time_rate: Option<f64>,
    /// Drop a previously set `--print-time-rate` and go back to the
    /// computed machine rate.
    #[arg(long = "no-print-time-rate")]
    no_print_time_rate: bool,
}

#[derive(Subcommand)]
pub enum PrinterCommand {
    /// Save a printer profile. Only `--name` is required; every numeric
    /// field falls back to its documented default (see
    /// `docs/reference/pricing.md`). Interactively, each prompt shows its
    /// default and a blank answer accepts it.
    Add {
        /// Profile name. Also the filename it's saved under, and what
        /// `--printer` looks up.
        #[arg(short, long)]
        name: Option<String>,
        #[command(flatten)]
        fields: PrinterFields,
    },
    /// Change a saved printer profile. Flags set fields; anything not
    /// passed keeps its saved value (interactively, each prompt shows
    /// the saved value and a blank answer keeps it). Editing a built-in
    /// preset saves a copy under the same name that shadows it.
    Edit {
        /// Profile name, as shown by `printcalc printer list`.
        name: String,
        #[command(flatten)]
        fields: PrinterFields,
    },
    /// Delete a saved printer profile. Built-in presets can't be
    /// removed; a saved profile shadowing one just stops shadowing it.
    #[command(visible_alias = "rm")]
    Remove {
        /// Profile name, as shown by `printcalc printer list`.
        name: String,
    },
    /// List every saved printer profile by name.
    List,
    /// Print a saved printer profile's values.
    Show {
        /// Profile name, as shown by `printcalc printer list`.
        name: String,
    },
}

pub fn run(cmd: PrinterCommand, interactive_mode: bool) -> Result<(), Box<dyn Error>> {
    match cmd {
        PrinterCommand::Add { name, fields } => {
            let name = interactive::require_string(name, "name", "Printer name", interactive_mode)?;
            let kind = parse_kind(fields.kind.as_deref())?.unwrap_or(PrinterKind::Fdm);
            let wear = resolve_wear(&fields, WearProfile::default(), interactive_mode)?;
            ops::add_printer(&PrinterProfile::new(name.clone(), kind, wear))?;
            println!("saved printer {name:?}");
            Ok(())
        }
        PrinterCommand::Edit { name, fields } => {
            let current = ops::show_printer(&name)?;
            let shadows_builtin = !ops::list_saved_printers()?.contains(&name);
            let kind = parse_kind(fields.kind.as_deref())?.unwrap_or(current.kind);
            let wear = resolve_wear(&fields, current.wear, interactive_mode)?;
            ops::add_printer(&PrinterProfile::new(name.clone(), kind, wear))?;
            if shadows_builtin {
                println!("saved printer {name:?} (shadows the built-in preset)");
            } else {
                println!("updated printer {name:?}");
            }
            Ok(())
        }
        PrinterCommand::Remove { name } => {
            ops::remove_printer(&name)?;
            println!("removed printer {name:?}");
            Ok(())
        }
        PrinterCommand::List => {
            for name in ops::list_printers()? {
                println!("{name}");
            }
            Ok(())
        }
        PrinterCommand::Show { name } => {
            let profile = ops::show_printer(&name)?;
            print_printer(&profile);
            Ok(())
        }
    }
}

fn parse_kind(kind: Option<&str>) -> Result<Option<PrinterKind>, Box<dyn Error>> {
    match kind {
        None => Ok(None),
        Some("fdm") => Ok(Some(PrinterKind::Fdm)),
        Some("resin") => Ok(Some(PrinterKind::Resin)),
        Some(other) => {
            Err(format!("invalid --kind {other:?} (expected \"fdm\" or \"resin\")").into())
        }
    }
}

/// Every wear field from `fields`, falling back to `base` — the
/// documented defaults for `add`, the saved profile for `edit`. Prompts
/// for each unset field when interactive, with the fallback shown.
fn resolve_wear(
    fields: &PrinterFields,
    base: WearProfile,
    interactive_mode: bool,
) -> Result<WearProfile, Box<dyn Error>> {
    let f = |value: Option<f64>, label: &str, default: f64| {
        interactive::defaulted_f64(value, label, default, interactive_mode)
    };
    let print_time_rate_override = if fields.no_print_time_rate {
        None
    } else {
        fields.print_time_rate.or(base.print_time_rate_override)
    };
    Ok(WearProfile::new()
        .with_purchase_price(f(
            fields.purchase_price,
            "Purchase price",
            base.purchase_price,
        )?)
        .with_additional_upfront_cost(f(
            fields.additional_upfront_cost,
            "Additional upfront cost",
            base.additional_upfront_cost,
        )?)
        .with_estimated_life_years(f(
            fields.estimated_life_years,
            "Estimated life (years)",
            base.estimated_life_years,
        )?)
        .with_estimated_uptime_percent(f(
            fields.estimated_uptime_percent,
            "Estimated uptime (0.0-1.0)",
            base.estimated_uptime_percent,
        )?)
        .with_annual_maintenance_cost(f(
            fields.annual_maintenance_cost,
            "Annual maintenance cost",
            base.annual_maintenance_cost,
        )?)
        .with_power_consumption_watts(f(
            fields.power_consumption_watts,
            "Power consumption (watts)",
            base.power_consumption_watts,
        )?)
        .with_cost_buffer_factor(f(
            fields.cost_buffer_factor,
            "Cost buffer factor",
            base.cost_buffer_factor,
        )?)
        .with_print_time_rate_override(print_time_rate_override))
}

fn print_printer(p: &PrinterProfile) {
    println!("name: {}", p.name);
    println!("kind: {:?}", p.kind);
    println!("purchase_price: {}", p.wear.purchase_price);
    println!(
        "additional_upfront_cost: {}",
        p.wear.additional_upfront_cost
    );
    println!("estimated_life_years: {}", p.wear.estimated_life_years);
    println!(
        "estimated_uptime_percent: {}",
        p.wear.estimated_uptime_percent
    );
    println!(
        "annual_maintenance_cost: {}",
        p.wear.annual_maintenance_cost
    );
    println!(
        "power_consumption_watts: {}",
        p.wear.power_consumption_watts
    );
    println!("cost_buffer_factor: {}", p.wear.cost_buffer_factor);
    if let Some(rate) = p.wear.print_time_rate_override {
        println!("print_time_rate_override: {rate}");
    }
}
