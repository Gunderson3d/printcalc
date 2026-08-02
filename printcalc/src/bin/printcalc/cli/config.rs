//! printcalc config command.

use std::error::Error;

use clap::Args;

use printcalc::model::cost::CostDefaults;

use crate::ops;

#[derive(Args)]
pub struct ConfigArgs {
    /// Default electricity cost per kWh.
    #[arg(short = 'e', long = "electricity-cost")]
    pub electricity_cost: Option<f64>,
    /// Default labor rate per hour.
    #[arg(short = 'r', long = "labor-rate")]
    pub labor_rate: Option<f64>,
    /// Comma-separated default markup percentages, e.g. `50,60,70`.
    #[arg(long = "markup")]
    pub markup: Option<String>,
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
    if let Some(csv) = args.markup {
        config.markup_percentages = csv
            .split(',')
            .map(|s| {
                s.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --markup entry {s:?}"))
            })
            .collect::<Result<Vec<f64>, _>>()?;
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
    println!("markup_percentages: {:?}", c.markup_percentages);
}
