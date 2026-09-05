# printcalc

The `printcalc` command-line tool: calculates 3D print job costs —
material, print time, printer wear, electricity, labor, and extra
hardware — from real sliced-file output instead of manual entry. All
parsing and cost-calculation logic lives in the `libprintcalc` crate;
this crate is the CLI built on it. `printcalc-wasm` in this workspace is
a thin browser frontend over the library (see `../SPEC.md` for the full
design).

## Install

```sh
cargo install printcalc        # build from source
cargo binstall printcalc       # or grab a prebuilt binary
```

`printcalc <file> [flags]` costs a sliced file (or calcs an existing job
`.toml`); `printer`/`filament` subcommands manage your profile library.
Run `printcalc --help` for the full flag list.

Sliced files already say which printer they were sliced for and what was
in each filament slot, so the common case needs no flags at all:

```console
$ printcalc Cube_0.2mm_PLA_Prusa\ MINI_20m7s.gcode
detected from Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode
  slicer:   OrcaSlicer 2.4.2
  printer:  Prusa MINI -> prusa-mini-plus (guess)
  slot 1 (Generic PLA @System #26A69A) -> priced from the file at $20.00/kg
...
```

Each guess is matched against your saved profiles first, then the
built-in presets; a slot that matches nothing but whose file states a
price is costed from that. `--printer`/`--filament`/`--filament-map`
override any of it, and `--no-detect` turns the whole thing off. Nothing
is ever priced at zero by default — an unidentifiable slot is an error
naming what the file said about it.

`-q/--quantity N` prices a batch: every cost and price shown is for the
whole batch, with the per-unit figure alongside it.

`printer add` and `filament add` only really need a name: every numeric
field falls back to the default tabulated in [PRICING.md](../PRICING.md)
(an 800 machine, 5-year life at 50% uptime, 160W, 1.3 buffer; filament at
20/kg with a 1.5 waste factor). The same defaults apply to fields a
hand-written profile `.toml` leaves out, so a profile can state only what
you actually know about it.

A key in a profile or job `.toml` that nothing recognizes is reported on
stderr and ignored, rather than failing the load: `warning: unrecognized
keys in printers/a1.toml: purchase_pric (ignored)`. It's worth saying
because the field it was meant to set silently took its default instead.

Sale prices are gross **margins**, not markups — a 60% margin is
`cost / 0.40`, i.e. 2.5x cost. `printcalc config --margin 50,60,70` sets
the tiers.

Format support is feature-gated the same way as in `libprintcalc`
(`bambu-3mf`, `gcode`, `prusa-bgcode`, `prusa-sl1`, with `bambu`/`prusa`
umbrellas; `default = ["bambu", "prusa"]`) and passed through when
building from source:

```sh
cargo install printcalc --no-default-features --features prusa-sl1
```

## Library

Writing an application instead of using the CLI? Depend on
[`libprintcalc`](../libprintcalc) — see its README and crate docs
(`cargo doc --open -p libprintcalc`) for the API.

## License

[Apache-2.0](../LICENSE)
