//! Shared prompt helpers.
//!
//! Each takes an already-parsed flag value and fills it in when absent:
//! by asking, when the terminal is interactive, or by failing or falling
//! back to a default under `--no-input`. Keeping that decision here means
//! no subcommand has to check the interactivity flag itself.

use std::error::Error;
use std::io::{self, Write};

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        // EOF (e.g. piped stdin ran out or was closed) — don't spin the
        // retry loop forever on an empty answer.
        println!();
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of input"));
    }
    Ok(line.trim().to_string())
}

fn missing(flag: &str) -> Box<dyn Error> {
    format!("missing required --{flag} (rerun without --no-input to be prompted)").into()
}

/// `value` if already given; otherwise prompts for it (or, under
/// `--no-input`, errors naming the missing flag).
pub fn require_string(
    value: Option<String>,
    flag: &str,
    label: &str,
    interactive: bool,
) -> Result<String, Box<dyn Error>> {
    if let Some(v) = value {
        return Ok(v);
    }
    if !interactive {
        return Err(missing(flag));
    }
    loop {
        let s = prompt(label)?;
        if !s.is_empty() {
            return Ok(s);
        }
    }
}

/// An f64 with a documented fallback. Prompts with the default shown and
/// accepts a blank answer when interactive; under `--no-input` it takes
/// the default rather than erroring.
pub fn defaulted_f64(
    value: Option<f64>,
    label: &str,
    default: f64,
    interactive: bool,
) -> io::Result<f64> {
    if let Some(v) = value {
        return Ok(v);
    }
    if !interactive {
        return Ok(default);
    }
    let label = format!("{label} [{default}]");
    loop {
        let s = prompt(&label)?;
        if s.is_empty() {
            return Ok(default);
        }
        match s.parse::<f64>() {
            Ok(v) => return Ok(v),
            Err(_) => println!("please enter a number, or leave blank for {default}"),
        }
    }
}

/// An optional f64 with no fallback needed at the call site — prompts once
/// (blank accepted) when interactive, otherwise just `None`.
pub fn optional_f64(value: Option<f64>, label: &str, interactive: bool) -> io::Result<Option<f64>> {
    if value.is_some() || !interactive {
        return Ok(value);
    }
    loop {
        let s = prompt(label)?;
        if s.is_empty() {
            return Ok(None);
        }
        match s.parse::<f64>() {
            Ok(v) => return Ok(Some(v)),
            Err(_) => println!("please enter a number, or leave blank"),
        }
    }
}

/// Same as [`require_string`], but lists `choices` first as a hint (e.g.
/// the printer/filament profiles already saved in the library).
pub fn require_choice(
    value: Option<String>,
    flag: &str,
    label: &str,
    interactive: bool,
    choices: &[String],
) -> Result<String, Box<dyn Error>> {
    if value.is_some() || !interactive {
        return require_string(value, flag, label, interactive);
    }
    if !choices.is_empty() {
        println!("available: {}", choices.join(", "));
    }
    require_string(None, flag, label, true)
}
