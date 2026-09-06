# Cost a multi-filament job

For AMS, IDEX, or any print where more than one slot was used, and each
slot needs its own price.

## Find the slot ids

Cost the file with no flags. The detected block lists every used slot
with the id the slicer gave it:

```console
$ printcalc single_plate_multicolor.gcode.3mf
detected from single_plate_multicolor.gcode.3mf
  slicer:   BambuStudio 02.07.01.62
  printer:  Bambu Lab P2S 0.4 nozzle -> bambu-p2s (guess)
  slot 1 (Bambu PLA Basic @BBL P2S #AE6342) -> bambu-pla-basic (exact)
  slot 2 (Bambu PLA Basic @BBL P2S #F8F4EF) -> bambu-pla-basic (exact)
```

Slot ids are the slicer's own — AMS tray number for Bambu, extruder index
for Prusa. Only slots that actually consumed material appear; a loaded
but unused tray is skipped.

## Map one filament per slot

```console
$ printcalc single_plate_multicolor.gcode.3mf \
    --filament-map 1=pla-grey,2=petg-red -m 5

material breakdown
  petg-red.................. 5.80g @ $24.00/kg = $0.14
  pla-grey.................. 7.25g @ $21.50/kg = $0.16

summary
  Material.................. $0.30
  ...
```

Each slot is priced against its own profile, so prices and waste factors
can differ per slot.

## Cover only some slots

Slots left out of `--filament-map` fall back to `--filament`:

```sh
printcalc part.gcode.3mf --filament-map 2=petg-red -f pla-grey
```

Slot 2 gets PETG; every other slot gets grey PLA.

## Let detection handle the ones you don't care about

`--filament-map` overrides detection only for the slots it names. Every
other slot still gets matched from the file:

```sh
printcalc part.gcode.3mf --filament-map 1=my-expensive-pla
```

Slot 1 is yours; the rest are auto-detected, and the detected block
reports only those.

## Handle a slot that can't be priced

If a slot matches no profile and its file states no price, costing fails
and names it:

```
slot 2 (Generic PLA Silk @System #FF6A13) -> no matching profile
error: no filament for slot 2 (Generic PLA Silk @System #FF6A13); pass
--filament as a default, or --filament-map slot=name,... per slot
```

Every unpriced slot is listed at once, so you can fix them in one pass
rather than rerunning per slot.

Three ways out:

- Map it: `--filament-map 2=<name>`.
- Give a catch-all: `-f <name>` covers every unmapped slot.
- Give a stored fallback: `--default-filament <name>` is kept in the job
  rather than expanded per slot, so it also covers slots that only show
  up when the file is re-parsed later.

Nothing is priced at zero by default. See
[why matching refuses to guess](../explanation/conservative-matching.md).

## Note on grouping

The material breakdown groups by *profile*, not by slot. Two slots
sharing a profile — the same filament in two trays — collapse into one
line, as in the first example above. The per-slot split is preserved in
the calculation; only the display aggregates.

## See also

- [Job files: materials](../reference/job-file.md#materials) — the TOML
  a mapping produces.
- [CLI reference](../reference/cli.md#profile-selection) — full
  precedence rules.
