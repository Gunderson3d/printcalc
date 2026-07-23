//! Shared prompt helpers. Interactive is the default across the whole CLI —
//! any required value not passed as a flag gets prompted for; `--no-input`
//! turns that off and fails fast instead, naming the missing flag.
//! See `SPEC.md` §7.

use std::error::Error;
use std::io::{self, Write};

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
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

pub fn require_f64(
    value: Option<f64>,
    flag: &str,
    label: &str,
    interactive: bool,
) -> Result<f64, Box<dyn Error>> {
    if let Some(v) = value {
        return Ok(v);
    }
    if !interactive {
        return Err(missing(flag));
    }
    loop {
        let s = prompt(label)?;
        match s.parse::<f64>() {
            Ok(v) => return Ok(v),
            Err(_) => println!("please enter a number"),
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
