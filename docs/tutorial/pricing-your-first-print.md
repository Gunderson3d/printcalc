# Pricing your first print

In this tutorial you will price a real print job, teach `printcalc` about
your own printer and filament, and save a quote you can reopen later. It
takes about ten minutes.

You will need a sliced file. Any of these work:

- a `.gcode` from PrusaSlicer, OrcaSlicer, SuperSlicer, or most other
  FDM slicers
- a `.gcode.3mf` from Bambu Studio
- a `.bgcode` from PrusaSlicer
- a `.sl1` or `.sl1s` from PrusaSlicer for resin

If you don't have one to hand, open your slicer, load any small model,
slice it, and export. A calibration cube is ideal.

The numbers you see will differ from the ones printed here — they depend
on your file and your hardware. The shape of the output will match.

## Step 1: Install

```sh
cargo install printcalc
```

Or, for a prebuilt binary:

```sh
cargo binstall printcalc
```

Check it worked:

```sh
printcalc --version
```

## Step 2: Price the file with no setup at all

Point `printcalc` at your sliced file and give it nothing else:

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

You gave it one argument and got a price. That is because your sliced
file already records which printer it was sliced for and what was loaded
in each slot, and `printcalc` reads it.

Look at the `detected` block. It is telling you three things:

- **`slicer`** — what produced the file.
- **`printer`** — the file said `Prusa MINI`, and that was matched to a
  built-in preset called `prusa-mini-plus`. The `(guess)` means the names
  were close but not identical.
- **`slot 1`** — no stored filament matched, but the file stated its own
  price, so that was used.

Those are guesses about *your* hardware made from *someone else's*
presets. Time to replace them.

## Step 3: Tell it about your printer

Depreciation is the part nobody tracks. `printcalc` works it out from
what the machine cost and how long you expect it to last.

Add yours, substituting your own numbers:

```console
$ printcalc printer add --name mini \
    --purchase-price 429 \
    --power-consumption-watts 80 \
    --annual-maintenance-cost 40
saved printer "mini"
```

Only `--name` was strictly required. Every value you left out took a
sensible default — a 5-year life at 50% uptime, and a 1.3 buffer factor
over the raw depreciation math.

Check what was saved:

```sh
printcalc printer show mini
```

Got a number wrong? `printcalc printer edit mini --purchase-price 449`
changes just that field.

## Step 4: Tell it about your filament

```console
$ printcalc filament add --name pla-grey \
    --price 21.50 \
    --vendor "Prusa Polymers" \
    --color grey
saved filament "pla-grey"
```

`--price` is per kilogram. You didn't set `--efficiency-factor`, so it
took the default of `1.5` — meaning you'll be charged for 50% more
filament than the slicer predicted, covering purge, skirts, and the
occasional failed print.

## Step 5: Price against your own profiles

Now name them, and add the two things the file can't know: how long you
spend handling the print, and how many you're making.

```console
$ printcalc Cube_0.2mm_PLA_Prusa\ MINI_20m7s.gcode \
    -p mini -f pla-grey -m 12 -q 5
detected from Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode
  slicer:   OrcaSlicer 2.4.2

material breakdown
  pla-grey.................. 23.25g @ $21.50/kg = $0.50

summary  (5 units)
  Material.................. $0.50
  Printer wear.............. $0.06
  Electricity............... $0.02
  Labor..................... $15.00
  Hardware.................. $0.00
  ─────────────────────────────
  TOTAL..................... $15.59
    per unit................ $3.12

  sale price @ 50% / 60% / 70% margin:  $31.17  $38.97  $51.96
  per unit:                             $6.23  $7.79  $10.39
```

Several things changed.

The `detected` block went quiet about the printer and the filament,
because `-p` and `-f` already answered those. It only reports what you
haven't decided yourself.

`-m 12` is twelve minutes of hands-on time **per print**, not per batch.
With `-q 5` that becomes an hour of labor, and labor now dominates the
job — which is the honest answer, and the one most spreadsheets miss.

Every figure is for all five units, with the per-unit figure below it.

## Step 6: Set your margins

The three prices at the bottom are gross margins: at 60%, cost is 40% of
the sale price. Set your own tiers:

```console
$ printcalc config --margin 40,55,65
saved config
electricity_cost_per_kwh: 0.14
labor_rate_per_hour: 15
margin_percentages: [40.0, 55.0, 65.0]
```

While you're here, set your real electricity price and labor rate:

```sh
printcalc config --electricity-cost 0.31 --labor-rate 22
```

These become the defaults for every job from now on.

## Step 7: Save the quote

Add `-o` to write the job to a file:

```console
$ printcalc Cube_0.2mm_PLA_Prusa\ MINI_20m7s.gcode \
    -p mini -f pla-grey -m 12 -q 5 -o cube-quote.toml
wrote cube-quote.toml
...
```

The same summary prints as before, after the `wrote` line. Re-cost it
for ten units by passing the job file back with a new quantity:

```sh
printcalc cube-quote.toml -q 10
```

The price updates. Open `cube-quote.toml` and you'll see plain, editable
TOML — `quantity`, labor, hardware and overrides are all there to change
by hand too. You never touched the sliced file — the job refers to
it by path and re-reads it each time.

## What you have now

- A printer profile and a filament profile, reusable across every future
  job.
- Your own margins, electricity price and labor rate as defaults.
- A saved quote you can edit and re-cost.

## Where to go next

- [Cost a multi-filament job](../how-to/cost-a-multi-filament-job.md) —
  AMS and INDX prints, priced per slot.
- [Correct a wrong detection](../how-to/correct-a-wrong-detection.md) —
  when the guess is wrong.
- [Save and reuse jobs](../how-to/save-and-reuse-jobs.md) — making a
  quote survive a moved file or a deleted profile.
- [Pricing reference](../reference/pricing.md) — every formula, in full.
