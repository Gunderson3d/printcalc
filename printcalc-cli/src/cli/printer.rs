//! printer subcommand

use std::error::Error;

use clap::{Args, Subcommand};

use printcalc_lib::model::printer::{PrinterKind, PrinterProfile, WearProfile};
use printcalc_lib::ops;

use crate::interactive;

#[derive(Args)]
pub struct PrinterArgs {
    #[command(subcommand)]
    pub command: PrinterCommand,
}

#[derive(Subcommand)]
pub enum PrinterCommand {
    Add {
        #[arg(short, long)]
        name: Option<String>,
        /// "fdm" or "resin".
        #[arg(short, long, default_value = "fdm")]
        kind: String,
        #[arg(long = "purchase-price")]
        purchase_price: Option<f64>,
        #[arg(long = "additional-upfront-cost", default_value_t = 0.0)]
        additional_upfront_cost: f64,
        #[arg(long = "estimated-life-years")]
        estimated_life_years: Option<f64>,
        #[arg(long = "estimated-uptime-percent")]
        estimated_uptime_percent: Option<f64>,
        #[arg(long = "annual-maintenance-cost")]
        annual_maintenance_cost: Option<f64>,
        #[arg(long = "power-consumption-watts")]
        power_consumption_watts: Option<f64>,
        #[arg(long = "cost-buffer-factor")]
        cost_buffer_factor: Option<f64>,
    },
    List,
    Show {
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
            let purchase_price = interactive::require_f64(
                purchase_price,
                "purchase-price",
                "Purchase price",
                interactive_mode,
            )?;
            let estimated_life_years = interactive::require_f64(
                estimated_life_years,
                "estimated-life-years",
                "Estimated life (years)",
                interactive_mode,
            )?;
            let estimated_uptime_percent = interactive::require_f64(
                estimated_uptime_percent,
                "estimated-uptime-percent",
                "Estimated uptime (0.0-1.0)",
                interactive_mode,
            )?;
            let annual_maintenance_cost = interactive::require_f64(
                annual_maintenance_cost,
                "annual-maintenance-cost",
                "Annual maintenance cost",
                interactive_mode,
            )?;
            let power_consumption_watts = interactive::require_f64(
                power_consumption_watts,
                "power-consumption-watts",
                "Power consumption (watts)",
                interactive_mode,
            )?;
            let cost_buffer_factor = interactive::require_f64(
                cost_buffer_factor,
                "cost-buffer-factor",
                "Cost buffer factor",
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
