# CLI reference

```
printcalc [OPTIONS] [FILE] [COMMAND]
```

Every command accepts `--no-input`, which never prompts and fails instead
if something required wasn't given as a flag. Use it in scripts.

## printcalc \<file\>

Costs a sliced file, or re-costs a saved job.

`FILE` is a sliced file or a job `.toml`. The choice is made by
extension: `.toml` is a job file, anything else is a sliced file.

With no other flags, `printcalc` reads the printer and filament the
slicer recorded and matches them against your profiles — see
[Auto-detection](#auto-detection).

### Profile selection

| Flag | Meaning |
|---|---|
| `-p, --printer <NAME>` | Printer profile to price wear and electricity against. Overrides detection. |
| `-f, --filament <NAME>` | Filament for every slot. Overrides detection. |
| `--filament-map <MAP>` | One filament per slot: `1=pla-black,2=petg-red`. Slots left out fall back to `--filament`. |
| `--default-filament <NAME>` | Filament for any slot still unmapped at cost time. Stored in the job rather than expanded per slot, so it also covers slots that appear only when the file is re-parsed later. |
| `--no-detect` | Ignore what the file says entirely, and ask (or fail) as if it said nothing. |

Precedence is `--filament-map`, then `--filament`, then detection, then a
prompt (or an error under `--no-input`).

`--filament` expands per slot when the job is saved, so the mapping is
visible in the file. `--default-filament` stays a single job-level
fallback. See [job files](job-file.md#materials).

### Job inputs

| Flag | Default | Meaning |
|---|---|---|
| `-n, --name <NAME>` | output stem, else input stem | A label for the job. |
| `-m, --labor-minutes <N>` | prompt | Hands-on time **per unit** — setup, removal, finishing. |
| `-r, --labor-rate <N>` | global config (`15.0`) | Labor rate per hour for this job. |
| `-q, --quantity <N>` | `1` | Units in the batch. Scales every cost. |
| `-H, --hardware <NAME:PRICE[:UNITS]>` | — | Extra per-unit cost — inserts, magnets, packaging. `M3 insert:0.08:4` is four at $0.08 each **per print**; units default to `1`. Repeatable; `--extra` is an alias. On a job `.toml`, adds to the file's existing entries. |
| `-P, --plate <P>` | `all` | `all` or a plate number. Ignored with a warning for single-plate formats. |

### Output

| Flag | Default | Meaning |
|---|---|---|
| `-o, --output <PATH>` | — | Save the job here. Without it nothing is written; the cost is computed and shown. |
| `-t, --type <TIER>` | `default` | How much to embed when saving: `default`, `full`, `bundled`. See [job tiers](../explanation/job-tiers.md). |
| `--retier <TIER>` | — | Re-tier an existing job `.toml` instead of costing it. Requires `-o`. |
| `-F, --format <FMT>` | `table` | `table` or `json`. |

`--format json` prints the full `CostBreakdown` on stdout. Warnings and
detection notes go to stderr, so the JSON is always clean to pipe.

### Auto-detection

Every supported format records which printer it was sliced for and what
sat in each slot. `printcalc` reads that and reports what it found:

```console
$ printcalc Cube_0.2mm_PLA_Prusa\ MINI_20m7s.gcode
detected from Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode
  slicer:   OrcaSlicer 2.4.2
  printer:  Prusa MINI -> prusa-mini-plus (guess)
  slot 1 (Generic PLA @System #26A69A) -> priced from the file at $20.00/kg
```

Names are matched against your saved profiles first, then the built-in
presets. A match is labelled `exact` only when the names agree
completely; anything else is a `guess`. A slot that matches nothing but
whose file states a price is costed from that price.

Detection stays quiet about anything a flag already covers, and prints to
stderr. A slot that ends up with no profile and no stated price is an
error naming what the file said about it — never a silent zero. See
[why matching refuses to guess](../explanation/conservative-matching.md).

## printcalc printer

Manages saved printer profiles. See
[profile files](profile-files.md#printer-profiles).

```
printcalc printer add [OPTIONS]
printcalc printer list
printcalc printer show <NAME>
```

`add` needs only `--name`; every numeric field falls back to its
[default](pricing.md#5-defaults). Interactively it prompts with each
default shown, and a blank answer accepts it.

| Flag | Default |
|---|---|
| `-n, --name <NAME>` | prompt |
| `-k, --kind <KIND>` | `fdm` (`fdm` or `resin`) |
| `--purchase-price <N>` | `800.0` |
| `--additional-upfront-cost <N>` | `0.0` |
| `--estimated-life-years <N>` | `5.0` |
| `--estimated-uptime-percent <N>` | `0.5` |
| `--annual-maintenance-cost <N>` | `80.0` |
| `--power-consumption-watts <N>` | `160.0` |
| `--cost-buffer-factor <N>` | `1.3` |
| `--print-time-rate <N>` | unset |

`--print-time-rate` replaces the entire computed machine rate —
depreciation, electricity and buffer factor alike.

## printcalc filament

Manages saved filament and resin profiles. See
[profile files](profile-files.md#filament-profiles).

```
printcalc filament add [OPTIONS]
printcalc filament list
printcalc filament show <NAME>
```

| Flag | Default |
|---|---|
| `-n, --name <NAME>` | prompt |
| `-k, --kind <KIND>` | `filament` (`filament` or `resin`) |
| `--price <N>` | `20.0` — per kg for filament, per liter for resin |
| `--efficiency-factor <N>` | `1.5` |
| `--color <TEXT>` | unset |
| `--vendor <TEXT>` | unset |

## printcalc config

Views or sets the global defaults in
`~/.config/printcalc/config.toml`. With no flags, prints the current
values.

| Flag | Default |
|---|---|
| `-e, --electricity-cost <N>` | `0.14` |
| `-r, --labor-rate <N>` | `15.0` |
| `--margin <LIST>` | `50,60,70` |

`--margin` takes comma-separated gross **margin** percentages — the share
of the sale price that isn't cost, so 60 prices at `cost / 0.40`. Values
of 100 or more are rejected. `--markup` is accepted as an alias, but its
numbers mean margins. See
[margins are not markups](../explanation/margins-not-markups.md).

## Environment

| Variable | Meaning |
|---|---|
| `PRINTCALC_CONFIG_DIR` | Overrides `~/.config/printcalc` — a second profile set, a portable location, or a throwaway library for scripting. |

## Exit behaviour

`printcalc` exits non-zero on any error and prints the reason to stderr.
Warnings — an unrecognized key in a file, a `--plate` on a single-plate
format — go to stderr but don't stop the run.
