//! Subcommand modules: one per top-level command.
//!
//! [`job`] is the default path — costing a sliced file or a saved job —
//! and is what runs when no subcommand is given. [`printer`] and
//! [`filament`] manage the profile library; [`config`] manages the
//! global defaults.
//!
//! The `clap` doc comments in these modules are the source of
//! `--help` text, so they are written for the terminal rather than for
//! rustdoc.

pub mod config;
pub mod filament;
pub mod job;
pub mod printer;

/// Parses a `--margin` list — comma-separated gross margin percentages —
/// shared by `printcalc config --margin` and the per-job `--margin`. A
/// margin is the share of the sale price that isn't cost, so anything
/// at or above 100 can't price and is rejected here rather than
/// dividing by zero later.
pub fn parse_margins(csv: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let margins = csv
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<f64>()
                .map_err(|_parse_err| format!("invalid --margin entry {s:?}"))
        })
        .collect::<Result<Vec<f64>, _>>()?;
    if let Some(bad) = margins.iter().find(|m| **m >= 100.0 || **m < 0.0) {
        return Err(format!(
            "margin of {bad}% is out of range — a margin is the share of the sale price that isn't cost, so it must be 0-99 (a 60% markup is a 37.5% margin)"
        )
        .into());
    }
    Ok(margins)
}
