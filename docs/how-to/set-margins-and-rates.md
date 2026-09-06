# Set margins and rates

For putting your own electricity price, labor rate and margin tiers in
front of the defaults.

## Set the global defaults

```sh
printcalc config --electricity-cost 0.31 --labor-rate 22 --margin 40,55,65
```

With no flags, `printcalc config` prints the current values:

```console
$ printcalc config
electricity_cost_per_kwh: 0.31
labor_rate_per_hour: 22
margin_percentages: [40.0, 55.0, 65.0]
```

These are saved to `~/.config/printcalc/config.toml` and apply to every
job that doesn't override them.

## Margins are not markups

`--margin 60` means cost is 40% of the sale price, so the price is
`cost / 0.40` — 2.5× cost. It is **not** `cost × 1.60`; that would be a
60% markup, which is only a 37.5% margin.

A margin of 100 or more is rejected. `--markup` is accepted as an alias
for compatibility, but its numbers mean margins. See
[margins are not markups](../explanation/margins-not-markups.md).

## Override for one job

Any of the three can be set per job without touching the global config.

Labor rate, at the command line:

```sh
printcalc widget.gcode -r 35 ...
```

Electricity and margins, in a saved job file:

```toml
electricity_cost_per_kwh_override = 0.48
margin_percentages_override = [45.0, 60.0]
```

```sh
printcalc widget.toml
```

## Charge a flat machine rate instead of deriving one

If you already know what an hour on a machine costs you, state it and
skip the depreciation math entirely:

```sh
printcalc printer add --name shop-rate --print-time-rate 2.50
```

`--print-time-rate` replaces the whole computed rate — depreciation,
electricity and the buffer factor alike. The job reports the entire
amount as printer wear and shows zero electricity, because a flat rate
says nothing about how it splits.

## Adjust the waste allowance

`efficiency_factor` on a filament profile is how much more material you
are charged for than the slicer predicted — purge, skirts, failures. The
default is `1.5`.

Per profile:

```sh
printcalc filament add --name pla-clean --price 21.50 --efficiency-factor 1.15
```

For one job, across every material at once, in the job file:

```toml
material_efficiency_factor_override = 1.2
```

## Use a separate set of defaults

`PRINTCALC_CONFIG_DIR` overrides the whole config directory — profiles
and config together:

```sh
PRINTCALC_CONFIG_DIR=~/quotes/client-a printcalc widget.gcode ...
```

Useful for a second business, a shared team profile set, or a throwaway
library in a test script.

## See also

- [Pricing reference](../reference/pricing.md) — every formula.
- [Profile files](../reference/profile-files.md#global-config).
