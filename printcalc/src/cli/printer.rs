//! `printcalc printer` — add, list and inspect saved printer profiles.
//!
//! `add` requires only a name: every numeric field falls back to
//! [`WearProfile::default`], prompting with that default shown when
//! interactive and taking it silently under `--no-input`.

use std::error::Error;

use clap::{Args, Subcommand};

use libprintcalc::model::printer::{
    DEFAULT_ADDITIONAL_UPFRONT_COST, DEFAULT_ANNUAL_MAINTENANCE_COST, DEFAULT_COST_BUFFER_FACTOR,
    DEFAULT_LIFE_YEARS, DEFAULT_POWER_CONSUMPTION_WATTS, DEFAULT_PURCHASE_PRICE,
    DEFAULT_UPTIME_PERCENT, PrinterKind, PrinterProfile, WearProfile,
};

use crate::interactive;
use crate::ops;

#[derive(Args)]
pub struct PrinterArgs {
    #[command(subcommand)]
    pub command: PrinterCommand,
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
        /// "fdm" or "resin".
        #[arg(short, long, default_value = "fdm")]
        kind: String,
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
        #[arg(long = "print-time-rate")]
        print_time_rate: Option<f64>,
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
        PrinterCommand::Add {
            name,
            kind,
            purchase_price,
            additional_upfront_cost,
            estimated_life_years,
            estimated_uptime_percent,
            annual_maintenance_cost,
            power_consumption_watts,
            cost_buffer_factor,
            print_time_rate,
        } => {
            let name = interactive::require_string(name, "name", "Printer name", interactive_mode)?;
            let kind = match kind.as_str() {
                "fdm" => PrinterKind::Fdm,
                "resin" => PrinterKind::Resin,
                other => {
                    return Err(format!(
                        "invalid --kind {other:?} (expected \"fdm\" or \"resin\")"
                    )
                    .into());
                }
            };
            let additional_upfront_cost = interactive::defaulted_f64(
                additional_upfront_cost,
                "Additional upfront cost",
                DEFAULT_ADDITIONAL_UPFRONT_COST,
                interactive_mode,
            )?;
            let purchase_price = interactive::defaulted_f64(
                purchase_price,
                "Purchase price",
                DEFAULT_PURCHASE_PRICE,
                interactive_mode,
            )?;
            let estimated_life_years = interactive::defaulted_f64(
                estimated_life_years,
                "Estimated life (years)",
                DEFAULT_LIFE_YEARS,
                interactive_mode,
            )?;
            let estimated_uptime_percent = interactive::defaulted_f64(
                estimated_uptime_percent,
                "Estimated uptime (0.0-1.0)",
                DEFAULT_UPTIME_PERCENT,
                interactive_mode,
            )?;
            let annual_maintenance_cost = interactive::defaulted_f64(
                annual_maintenance_cost,
                "Annual maintenance cost",
                DEFAULT_ANNUAL_MAINTENANCE_COST,
                interactive_mode,
            )?;
            let power_consumption_watts = interactive::defaulted_f64(
                power_consumption_watts,
                "Power consumption (watts)",
                DEFAULT_POWER_CONSUMPTION_WATTS,
                interactive_mode,
            )?;
            let cost_buffer_factor = interactive::defaulted_f64(
                cost_buffer_factor,
                "Cost buffer factor",
                DEFAULT_COST_BUFFER_FACTOR,
                interactive_mode,
            )?;

            let profile = PrinterProfile {
                name: name.clone(),
                kind,
                wear: WearProfile {
                    purchase_price,
                    additional_upfront_cost,
                    estimated_life_years,
                    estimated_uptime_percent,
                    annual_maintenance_cost,
                    power_consumption_watts,
                    cost_buffer_factor,
                    print_time_rate_override: print_time_rate,
                },
            };
            ops::add_printer(&profile)?;
            println!("saved printer {name:?}");
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
}
