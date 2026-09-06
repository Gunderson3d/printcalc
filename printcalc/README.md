# printcalc

Command-line tool for calculating 3D print job costs — material, print
time, printer wear, electricity, labor and hardware — from real
sliced-file output instead of manual entry.

```console
$ printcalc Cube_0.2mm_PLA_Prusa\ MINI_20m7s.gcode
detected from Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode
  slicer:   OrcaSlicer 2.4.2
  printer:  Prusa MINI -> prusa-mini-plus (guess)
  slot 1 (Generic PLA @System #26A69A) -> priced from the file at $20.00/kg

material breakdown
  Generic PLA @System.............. 4.65g @ $20.00/kg = $0.09

summary
  Material......................... $0.09
  Printer wear..................... $0.02
  Electricity...................... $0.01
  Labor............................ $0.00
  Hardware......................... $0.00
  ────────────────────────────────────
  TOTAL............................ $0.12

  sale price @ 50% / 60% / 70% margin:  $0.23  $0.29  $0.38
```

Sliced files already record which printer they were sliced for and what
was in each filament slot, so the common case needs no flags at all.

## Install

```sh
cargo install printcalc     # build from source
cargo binstall printcalc    # or a prebuilt binary
```

## Usage

```sh
printcalc <file>                    # cost a sliced file or a job .toml
printcalc <file> -o job.toml        # and save the job
printcalc printer add --name mini   # manage the profile library
printcalc filament list
printcalc config --margin 40,55,65  # global defaults
```

`printcalc --help` lists every flag; the
[CLI reference](../docs/reference/cli.md) explains them.

New here? **[Pricing your first
print](../docs/tutorial/pricing-your-first-print.md)** goes from install
to a saved quote in about ten minutes.

## Documentation

- [Tutorial](../docs/tutorial/pricing-your-first-print.md)
- [How-to guides](../docs/how-to/) — multi-filament jobs, batches,
  correcting a wrong detection, saving quotes, setting margins
- [Reference](../docs/reference/) — CLI, pricing, job files, profile
  files, formats
- [Explanation](../docs/explanation/) — why margins, how matching
  decides, job tiers, architecture

## Things worth knowing up front

**Prices are gross margins, not markups.** A 60% margin is `cost / 0.40`,
i.e. 2.5× cost. See [margins are not
markups](../docs/explanation/margins-not-markups.md).

**Nothing is priced at zero by default.** A filament slot that can't be
identified is an error naming what the file said about it, not a silent
zero. See [why matching refuses to
guess](../docs/explanation/conservative-matching.md).

**Profiles need only a name.** `printer add` and `filament add` fill in
every numeric field you skip with a
[documented default](../docs/reference/pricing.md#5-defaults).

## Formats

| Format | Extension | Feature |
|---|---|---|
| Bambu Studio 3MF | `.gcode.3mf` | `bambu-3mf` |
| Plain-text G-code | `.gcode` | `gcode` |
| PrusaSlicer binary G-code | `.bgcode` | `prusa-bgcode` |
| PrusaSlicer SLA | `.sl1`, `.sl1s` | `prusa-sl1` |

`bambu` and `prusa` are umbrella features; `default = ["bambu", "prusa"]`
builds everything. For a smaller binary:

```sh
cargo install printcalc --no-default-features --features prusa-sl1
```

## Library

Building an application rather than using the CLI? Depend on
[`libprintcalc`](../libprintcalc) — see [Embed the
library](../docs/how-to/embed-the-library.md).

## License

[Apache-2.0](../LICENSE)
