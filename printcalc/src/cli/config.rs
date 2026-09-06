//! `printcalc config` — view and set the global defaults every job falls
//! back to when it overrides none of them, saved to
//! `<config dir>/config.toml`.
//!
//! With no flags the command prints the current values. A missing config
//! file is not an error: a fresh install uses the built-in defaults, so
//! there is no init step.

use std::error::Error;

use clap::Args;

use libprintcalc::model::cost::CostDefaults;

use crate::ops;

#[derive(Args)]
pub struct ConfigArgs {
    /// Default electricity cost per kWh, priced against each printer's
    /// stated wattage. [default: 0.14]
    #[arg(short = 'e', long = "electricity-cost")]
    pub electricity_cost: Option<f64>,
    /// Default labor rate per hour, used by any job that doesn't set
    /// its own with `--labor-rate`. [default: 15]
    #[arg(short = 'r', long = "labor-rate")]
    pub labor_rate: Option<f64>,
    /// Comma-separated default gross *margin* percentages, e.g.
    /// `50,60,70` — the share of the sale price that isn't cost, so a
    /// 60% margin prices at cost / 0.40, i.e. 2.5x cost. Not markups: a
    /// 60% markup would be 1.6x cost, which is only a 37.5% margin.
    /// Values of 100 or more are rejected. [default: 50,60,70]
    #[arg(long = "margin", alias = "markup")]
    pub margin: Option<String>,
}

pub fn run(args: ConfigArgs) -> Result<(), Box<dyn Error>> {
    let mut config = ops::show_config()?;
    let mut changed = false;

    if let Some(v) = args.electricity_cost {
        config.electricity_cost_per_kwh = v;
        changed = true;
    }
    if let Some(v) = args.labor_rate {
        config.labor_rate_per_hour = v;
        changed = true;
    }
    if let Some(csv) = args.margin {
        let margins = csv
            .split(',')
            .map(|s| {
                s.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --margin entry {s:?}"))
            })
            .collect::<Result<Vec<f64>, _>>()?;
        if let Some(bad) = margins.iter().find(|m| **m >= 100.0 || **m < 0.0) {
            return Err(format!(
                "margin of {bad}% is out of range — a margin is the share of the sale price that isn't cost, so it must be 0-99 (a 60% markup is a 37.5% margin)"
            )
            .into());
        }
        config.margin_percentages = margins;
        changed = true;
    }

    if changed {
        ops::set_config(&config)?;
        println!("saved config");
    }
    print_config(&config);
    Ok(())
}

fn print_config(c: &CostDefaults) {
    println!("electricity_cost_per_kwh: {}", c.electricity_cost_per_kwh);
    println!("labor_rate_per_hour: {}", c.labor_rate_per_hour);
    println!("margin_percentages: {:?}", c.margin_percentages);
}
