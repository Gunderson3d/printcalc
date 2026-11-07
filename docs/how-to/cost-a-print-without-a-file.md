# Cost a print without a sliced file

For quoting from a slicer's summary screen, or a print someone describes,
when there's no file to hand.

## Type in the time and the material

Leave out the file and give the print time and the filament used:

```sh
printcalc --print-time 1h30m --grams 50 -p bambu-a1 -f bambu-pla-basic
```

```
material breakdown
  bambu-pla-basic.............. 75.00g @ $24.99/kg = $1.87

summary
  Material..................... $1.87
  Printer wear................. $0.12
  Electricity.................. $0.04
  Labor........................ $0.00
  Hardware..................... $0.00
  ────────────────────────────────
  TOTAL........................ $2.04

  sale price @ 50% / 60% / 70% margin:  $4.08  $5.10  $6.80
```

The 50g you typed became 75g because `bambu-pla-basic` allows 1.5× for
waste, the same as it would for a sliced file.

`--print-time` takes `1h30m`, `45m`, `90s`, `1.5h` or `1:30`. A bare
number is rejected, since it could be hours or minutes.

With prompts on, a missing `--print-time` or `--grams` is asked for. Under
`--no-input` it's an error naming the flag.

## More than one material

Give each slot its amount and map each slot to a profile:

```sh
printcalc --print-time 1h30m --grams 1=40,2=12 \
  -p bambu-a1 --filament-map 1=bambu-pla-basic,2=bambu-petg-basic
```

The slot names are yours to choose. They only need to match between
`--grams` and `--filament-map`.

## Resin

Use `--ml` instead of `--grams`, with a resin printer and resin profile:

```sh
printcalc --print-time 2:15 --ml 30 -p prusa-sl1s-speed -f generic-resin-grey
```

A print is either filament or resin, so `--grams` and `--ml` can't be
combined. Pricing an FDM printer against `--ml`, or the reverse, is an
error.

## Save it

Every other job flag works as it does with a file: `-q`, `-m`, `-H`,
`--margin`, `-o` and the rest. A saved job stores the numbers you typed
as an embedded summary (`source_format = "manual-fdm"` or
`"manual-resin"`), since there's no file to point back to:

```sh
printcalc --print-time 45m --grams 20 -p bambu-a1 -f bambu-pla-basic -o bracket.toml
printcalc bracket.toml -q 10
```

There's nothing to auto-detect, so `--printer` and `--filament` (or
their prompts) are always needed.

## See also

- [CLI reference](../reference/cli.md#without-a-sliced-file)
- [Save and reuse jobs](save-and-reuse-jobs.md)
