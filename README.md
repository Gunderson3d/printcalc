# printcalc

Calculate what a 3D print actually costs, from the sliced file.

Material, print time, printer depreciation, electricity, labor and
hardware — read out of the file your slicer already produced, not typed
into a spreadsheet.

```console
$ printcalc "PlayBook'd Billiards.gcode.3mf"
plates
  Plate 1........................... 4h41m35s · 272.32g
  Plate 2........................... 2h26m34s · 114.89g
  Plate 3........................... 3h43m28s · 248.38g
  Plate 4........................... 12h49m38s · 351.50g
  Plate 5........................... 1h53m26s · 98.50g
  Plate 6........................... 30m30s · 10.20g
  Plate 7........................... 44m05s · 6.14g
  Plate 8........................... 1h21m58s · 10.90g
  Plate 9........................... 1h10m24s · 11.72g
  ─────────────────────────────────────
  Combined.......................... 29h21m38s · 1124.55g

material breakdown
  Generic PLA @BBL P2S.............. 391.31g @ $20.00/kg = $7.83
  bambu-pla-matte................... 1153.21g @ $24.99/kg = $28.82

summary
  Material.......................... $36.65
  Printer wear...................... $0.13
  Electricity....................... $0.68
  Labor............................. $0.00
  Hardware.......................... $0.00
  ─────────────────────────────────────
  TOTAL............................. $37.45

  sale price @ 50% / 60% / 70% margin:  $74.90  $93.62  $124.83
```

One argument, no configuration. Sliced files record which printer they
were sliced for and what was in each slot, so `printcalc` starts from
what the file already knows and asks you only for what it can't.  
> [!NOTE]  
> Only tested with PrusaSlicer, OrcaSlicer, and Bambu Studio files
## Install

```sh
cargo install printcalc     # build from source
cargo binstall printcalc    # or a prebuilt binary
```

Shell completions and a man page come from the binary itself:
`printcalc completions zsh`, `printcalc man`. See the
[CLI reference](docs/reference/cli.md#printcalc-completions).

Try it on the web: **<https://gunderson3d.github.io/printcalc/>** runs
the same calculation in your browser, with the same built-in presets.
No files are sent to the server.

## Start here

**[Pricing your first print](docs/tutorial/pricing-your-first-print.md)**
— ten minutes, from install to a saved quote priced against your own
printer and filament.

## Documentation

**Tutorial** — learning by doing

- [Pricing your first print](docs/tutorial/pricing-your-first-print.md)

**How-to guides** — solving a specific problem

- [Cost a multi-filament job](docs/how-to/cost-a-multi-filament-job.md)
- [Price a batch](docs/how-to/price-a-batch.md)
- [Cost a print without a sliced file](docs/how-to/cost-a-print-without-a-file.md)
- [Correct a wrong detection](docs/how-to/correct-a-wrong-detection.md)
- [Save and reuse jobs](docs/how-to/save-and-reuse-jobs.md)
- [Set margins and rates](docs/how-to/set-margins-and-rates.md)
- [Embed the library in a Rust program](docs/how-to/embed-the-library.md)
- [Run it in the browser](docs/how-to/run-it-in-the-browser.md)

**Reference** — the details

- [CLI reference](docs/reference/cli.md)
- [Pricing reference](docs/reference/pricing.md)
- [Job files](docs/reference/job-file.md)
- [Profile files](docs/reference/profile-files.md)
- [Supported formats](docs/reference/supported-formats.md)
- Library API: `cargo doc --open -p libprintcalc`

**Explanation** — why it works the way it does

- [Margins are not markups](docs/explanation/margins-not-markups.md)
- [Why matching refuses to guess](docs/explanation/conservative-matching.md)
- [Job tiers](docs/explanation/job-tiers.md)
- [Architecture](docs/explanation/architecture.md)

## Supported formats

| Format | Extension | Feature |
|---|---|---|
| Bambu Studio 3MF | `.gcode.3mf` | `bambu-3mf` |
| Plain-text G-code | `.gcode` | `gcode` |
| PrusaSlicer binary G-code | `.bgcode` | `prusa-bgcode` |
| PrusaSlicer SLA | `.sl1`, `.sl1s` | `prusa-sl1` |

All four are built by default. Build a smaller binary with only what you
need:

```sh
cargo install printcalc --no-default-features --features prusa-sl1
```

## The workspace

| Crate | What it is |
|---|---|
| [`printcalc`](printcalc) | The command-line tool. |
| [`libprintcalc`](libprintcalc) | The library: parsing, matching, costing. No I/O in the core. |
| [`printcalc-wasm`](printcalc-wasm) | `wasm-bindgen` bindings for the browser. |

`printcalc` and `printcalc-wasm` are thin frontends over the same
library, so they cannot disagree about what a job costs. See
[Architecture](docs/explanation/architecture.md).

## A note on prices

Suggested sale prices are gross **margins**, not markups. A 60% margin is
`cost / 0.40` — 2.5× cost, not 1.6× cost. The two are frequently
confused, usually expensively; see
[Margins are not markups](docs/explanation/margins-not-markups.md).

## License

[Apache-2.0](LICENSE)
