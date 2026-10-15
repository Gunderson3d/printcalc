# Embed the library in a Rust program

For building your own tool on `libprintcalc` instead of shelling out to
the CLI.

```sh
cargo add libprintcalc
```

Pick only the formats you need:

```sh
cargo add libprintcalc --no-default-features --features gcode
```

## Price a file with as little input as possible

`auto_job` reads the printer and filament out of the file itself and
builds a costable job:

```rust
use libprintcalc::{auto_job, calculate, parse_slice_bytes, CostDefaults};

let slice = parse_slice_bytes(&bytes, "widget.gcode.3mf")?;
let auto = auto_job("widget", slice.clone(), slice, [], [], 1.5);
let breakdown = calculate(&auto.job, &CostDefaults::default())?;
println!("{:.2}", breakdown.total_cost);
```

The two `slice` arguments are the job's source and the parsed data: pass
the summary twice to embed it, or pass a path as the first to have it
re-read at cost time.

The last argument is the waste allowance used for a slot priced from the
file's own stated price, since no slicer records one.

## Show the guesses before trusting them

Nothing `auto_job` decides is authoritative. Every match comes back with
a confidence and what it matched on, so you can present it:

```rust
if let Some(m) = &auto.printer {
    println!("printer {} ({:.2}) via {}", m.profile.name, m.confidence, m.matched_on);
}

for slot in &auto.slots {
    match slot.profile() {
        Some(p) => println!("slot {} -> {}", slot.usage.slot_id, p.name),
        None => println!("slot {} unpriced", slot.usage.label()),
    }
}
```

Pass your user's own saved profiles as the two candidate iterators and
they are searched before the built-in presets:

```rust
let auto = auto_job("widget", slice.clone(), slice, &my_printers, &my_filaments, 1.5);
```

A slot that matches nothing and states no price is deliberately left out
of `auto.job.material`, so `calculate` fails with
`CalcError::MissingMaterial` rather than pricing it at zero.

## Build a job explicitly

When you already know which profiles to use:

```rust
use std::collections::HashMap;
use libprintcalc::{
    calculate_cost, create_material, create_printer, new_job, CostDefaults,
    MaterialKind, PrinterKind, ProfileRef, WearProfile, SINGLE_MATERIAL_SLOT,
};

let printer = create_printer(
    "mini",
    PrinterKind::Fdm,
    WearProfile::new()
        .with_purchase_price(429.0)
        .with_power_consumption_watts(80.0),
);
let filament = create_material("pla-grey", MaterialKind::Filament, 21.50, 1.5, None, None);

let job = new_job(
    "widget",
    slice.clone(),
    printer.clone(),
    HashMap::from([(
        SINGLE_MATERIAL_SLOT.to_string(),
        ProfileRef::Embedded { profile: Box::new(filament.clone()) },
    )]),
    None,   // labor
    vec![], // hardware
);

let breakdown = calculate_cost(
    &job,
    &slice,
    &printer,
    &HashMap::from([(SINGLE_MATERIAL_SLOT.to_string(), filament)]),
    None, // default_material
    &CostDefaults::default(),
)?;
```

`WearProfile::new()` starts from the
[documented defaults](../reference/pricing.md#5-defaults); each `with_*`
setter overrides one field.

## Construct types, don't spell them out

Every public struct in `libprintcalc` is `#[non_exhaustive]`, so a struct
literal or `..Default::default()` update won't compile from outside the
crate. Build values with their constructors and `with_*` setters
(`Job::new`, `MaterialProfile::new`, `PrinterProfile::new`,
`WearProfile::new`, `LaborCost::new`, `HardwareItem::new`,
`SliceSummary::new`, `PlateSummary::new`, `FilamentUsage::grams`/`ml`,
`CostDefaults::new`). Fields stay public, so reading and assigning them
directly still works. This is what lets a minor release add a field
without breaking your build.

`Job::add_hardware` and `Job::with_hardware` keep hardware names unique:
an item whose name is already present replaces that entry in place.
Pushing onto `job.hardware` directly bypasses this.

The public enums `MaterialKind`, `PrinterKind`, `JobTier` and
`SourceFormat` are `#[non_exhaustive]` too, so a `match` on one needs a
wildcard arm. `ProfileRef` and `JobSource` are deliberately not: a new
variant there would change what a job *is*, and that should be a visible
breaking change.

## Choose between `calculate` and `calculate_cost`

| | Resolves `Linked` refs | Reads files | Use when |
|---|---|---|---|
| `calculate` | yes, against built-in presets | yes, for a `Path` source | you have a `Job` and want a number |
| `calculate_cost` | no — you supply resolved profiles | no | you own profile lookup, or there is no filesystem |

`calculate_cost` does no I/O at all. That is what makes the library work
under WebAssembly.

## Warn about typos in user-supplied TOML

`parse_toml` returns the parsed value plus every key nothing claimed:

```rust
use libprintcalc::{parse_toml, PrinterProfile};

let (printer, unknown) = parse_toml::<PrinterProfile>(&text)?;
if !unknown.is_empty() {
    eprintln!("warning: ignored keys: {}", unknown.join(", "));
}
```

Worth doing: a typo'd `purchase_pric` silently takes the default, so
without the warning the job just costs wrong. The plain `parse_printer`,
`parse_material` and `parse_job` discard the list.

## Load from disk

The `disk` helpers read; they never write. Saving profiles and jobs is
the CLI's job.

```rust
let slice = libprintcalc::read_slice_file("widget.gcode.3mf".as_ref())?;
let job = libprintcalc::read_job("widget.toml".as_ref())?;
let printer = libprintcalc::read_printer("mini.toml".as_ref())?;
```

## See also

- `cargo doc --open -p libprintcalc` — the full API reference.
- [Architecture](../explanation/architecture.md) — why the library does
  no writing.
- [Run it in the browser](run-it-in-the-browser.md).
