# libprintcalc

Library for calculating 3D print job costs — material, print time,
printer wear, electricity, labor and hardware — from real sliced-file
output instead of manual entry.

The [`printcalc`](../printcalc) crate is the command-line tool built on
it; [`printcalc-wasm`](../printcalc-wasm) is a browser frontend over the
same code.

```sh
cargo add libprintcalc
```

## Quick start

`auto_job` reads the printer and filament out of the sliced file itself
and builds a costable job:

```rust
use libprintcalc::{auto_job, calculate, parse_slice_bytes, CostDefaults};

let slice = parse_slice_bytes(&bytes, "widget.gcode.3mf")?;
let auto = auto_job("widget", slice.clone(), slice, [], [], 1.5);
let breakdown = calculate(&auto.job, &CostDefaults::default())?;
println!("{:.2}", breakdown.total_cost);
```

Every match `auto_job` makes is a guess, returned with its confidence and
what it matched on so you can show it and let the user correct it. A slot
that matches nothing and states no price is left out of the job, so
costing fails naming it rather than pricing it at zero.

See [Embed the library](../docs/how-to/embed-the-library.md) for building
jobs explicitly, supplying your own candidate profiles, and reporting
unrecognized keys in user-supplied TOML.

## Entry points

Everything is re-exported at the crate root.

| | |
|---|---|
| `parse_slice_bytes` | sliced-file bytes → `SliceSummary` |
| `parse_job`, `parse_printer`, `parse_material` | TOML → model types |
| `parse_toml` | the same, plus every key nothing claimed |
| `read_slice_file`, `read_job`, `read_printer`, `read_material` | the same, from a path |
| `create_printer`, `create_material` | build profiles in code |
| `new_job` | a job from a source and explicit profiles |
| `auto_job` | a job from the sliced file alone |
| `calculate` | cost a job, resolving refs |
| `calculate_cost` | the pure core, given resolved profiles |

`calculate_cost` and `parse_slice_bytes` do no I/O at all. The `disk`
helpers read; nothing in this crate writes — saving is the CLI's job. See
[Architecture](../docs/explanation/architecture.md).

## Formats

| Format | Extension | Feature |
|---|---|---|
| Bambu Studio 3MF | `.gcode.3mf` | `bambu-3mf` |
| Plain-text G-code | `.gcode` | `gcode` |
| PrusaSlicer binary G-code | `.bgcode` | `prusa-bgcode` |
| PrusaSlicer SLA | `.sl1`, `.sl1s` | `prusa-sl1` |

`bambu` and `prusa` are umbrella features pulling in every format for
that vendor; `default = ["bambu", "prusa"]` builds all four. Take only
what you need:

```sh
cargo add libprintcalc --no-default-features --features prusa-sl1
```

Every parser produces the same `SliceSummary`, so nothing downstream
knows which format it came from. What each one records is tabulated in
[Supported formats](../docs/reference/supported-formats.md).

## Multi-plate and multi-filament

Bambu `.gcode.3mf` plates can be costed individually or together. Any
format with more than one filament slot — Bambu AMS, Prusa XL and Core
One IDEX — is priced per slot against that slot's own profile.

## Documentation

- [API reference](https://docs.rs/libprintcalc) — or `cargo doc --open -p libprintcalc`
- [Embed the library](../docs/how-to/embed-the-library.md)
- [Pricing reference](../docs/reference/pricing.md) — every formula
- [Why matching refuses to guess](../docs/explanation/conservative-matching.md)
- [Architecture](../docs/explanation/architecture.md)

## License

[Apache-2.0](../LICENSE)
