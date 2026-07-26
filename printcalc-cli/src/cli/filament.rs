//! filament subcommand
use std::error::Error;

use clap::{Args, Subcommand};

use printcalc_lib::model::filament::{MaterialKind, MaterialProfile};

use crate::interactive;
use crate::ops;

#[derive(Args)]
pub struct FilamentArgs {
    #[command(subcommand)]
    pub command: FilamentCommand,
}

#[derive(Subcommand)]
pub enum FilamentCommand {
    Add {
        #[arg(short, long)]
        name: Option<String>,
        /// "filament" or "resin".
        #[arg(short, long, default_value = "filament")]
        kind: String,
        #[arg(long = "price-per-kg")]
        price_per_kg: Option<f64>,
        #[arg(long = "price-per-liter")]
        price_per_liter: Option<f64>,
        #[arg(long = "density")]
        density_g_per_cm3: Option<f64>,
        #[arg(long = "efficiency-factor", default_value_t = 1.1)]
        efficiency_factor: f64,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        vendor: Option<String>,
    },
    List,
    Show {
        name: String,
    },
}

pub fn run(cmd: FilamentCommand, interactive_mode: bool) -> Result<(), Box<dyn Error>> {
    match cmd {
        FilamentCommand::Add {
            name,
            kind,
            price_per_kg,
            price_per_liter,
            density_g_per_cm3,
            efficiency_factor,
            color,
            vendor,
        } => {
            let name =
                interactive::require_string(name, "name", "Filament name", interactive_mode)?;
            let kind = match kind.as_str() {
                "filament" => MaterialKind::Filament,
                "resin" => MaterialKind::Resin,
                other => {
                    return Err(format!(
                        "invalid --kind {other:?} (expected \"filament\" or \"resin\")"
                    )
                    .into());
                }
            };
            let (price_per_kg, price_per_liter) = match (price_per_kg, price_per_liter) {
                (None, None) if interactive_mode => {
                    let kg = interactive::optional_f64(
                        None,
                        "Price per kg (blank to enter price per liter instead)",
                        true,
                    )?;
                    match kg {
                        Some(v) => (Some(v), None),
                        None => (
                            None,
                            Some(interactive::require_f64(
                                None,
                                "price-per-liter",
                                "Price per liter",
                                true,
                            )?),
                        ),
                    }
                }
                (None, None) => {
                    return Err("one of --price-per-kg or --price-per-liter is required".into());
                }
                other => other,
            };
            let profile = MaterialProfile {
                name: name.clone(),
                kind,
                price_per_kg,
                price_per_liter,
                density_g_per_cm3,
                efficiency_factor,
                color,
                vendor,
            };
            ops::add_filament(&profile)?;
            println!("saved filament {name:?}");
            Ok(())
        }
        FilamentCommand::List => {
            for name in ops::list_filaments()? {
                println!("{name}");
            }
            Ok(())
        }
        FilamentCommand::Show { name } => {
            let profile = ops::show_filament(&name)?;
            print_filament(&profile);
            Ok(())
        }
    }
}

fn print_filament(f: &MaterialProfile) {
    println!("name: {}", f.name);
    println!("kind: {:?}", f.kind);
    if let Some(p) = f.price_per_kg {
        println!("price_per_kg: {p}");
    }
    if let Some(p) = f.price_per_liter {
        println!("price_per_liter: {p}");
    }
    if let Some(d) = f.density_g_per_cm3 {
        println!("density_g_per_cm3: {d}");
    }
    println!("efficiency_factor: {}", f.efficiency_factor);
    if let Some(c) = &f.color {
        println!("color: {c}");
    }
    if let Some(v) = &f.vendor {
        println!("vendor: {v}");
    }
}
