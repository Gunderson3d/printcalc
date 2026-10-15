# Set margins and rates

For putting your own electricity price, labor rate and margin tiers in
front of the defaults.

## Set the global defaults

```sh
printcalc config --electricity-cost 0.31 --labor-rate 22 --margin 40,55,65 --currency "€"
```

With no flags, `printcalc config` prints the current values:

```console
$ printcalc config
electricity_cost_per_kwh: 0.31
labor_rate_per_hour: 22
margin_percentages: [40.0, 55.0, 65.0]
currency_symbol: "€"
```

`--currency` only changes the symbol in front of each amount. Nothing is
converted; your profiles and the output are in whatever currency you
entered them in.

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

Any of the three can be set per job without touching the global config:

```sh
printcalc widget.gcode -r 35 -e 0.48 --margin 45,60 ...
```

The same flags work on a saved job, for that run only, or written back
with `-o`:

```sh
printcalc widget.toml --margin 45,60                  # just this calc
printcalc widget.toml --margin 45,60 -o widget.toml   # and save it
```

In the job file they appear as:

```toml
electricity_cost_per_kwh_override = 0.48
margin_percentages_override = [45.0, 60.0]
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

For one job, across every material at once:

```sh
printcalc widget.gcode --efficiency-factor 1.2 ...
```

which a saved job stores as:

```toml
material_efficiency_factor_override = 1.2
```

## Change a saved profile

Filament prices move. Edit the profile instead of re-adding it:

```sh
printcalc filament edit pla-grey --price 23.90
printcalc printer edit mini --power-consumption-watts 90
printcalc filament remove old-brand
```

Anything not passed keeps its saved value. Editing a built-in preset
saves a copy under the same name that shadows it; removing that copy
brings the preset back.

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
