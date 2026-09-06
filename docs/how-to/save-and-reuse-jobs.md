# Save and reuse jobs

For keeping a quote you can reopen, and making it survive a moved file or
a changed profile.

## Save a job

```sh
printcalc widget.gcode -p mini -f pla-grey -m 12 -o widget.toml
```

Re-cost it any time:

```sh
printcalc widget.toml
```

The saved job refers to the sliced file by path and to your profiles by
name, so re-costing it picks up any change to either. That is usually
what you want while a quote is live.

## Understand what a saved job depends on

By default a job holds references, not data:

```toml
[source]
path = "widget.gcode"

[printer]
ref = "mini"

[material]
ref = "pla-grey"
```

This job breaks if the sliced file moves or the `mini` profile is
deleted. Relative paths resolve against **the job file's own directory**,
so moving the job and its sliced file together is fine.

## Freeze the profiles

`--retier full` resolves every `ref` into embedded profile data:

```sh
printcalc widget.toml --retier full -o widget-full.toml
```

```toml
[printer.profile]
name = "mini"
kind = "fdm"
purchase_price = 429.0
# ... the whole profile
```

The job now costs identically on a machine with no profile library. It
still needs the sliced file.

## Freeze everything

`--retier bundled` also embeds the parsed slice data:

```sh
printcalc widget.toml --retier bundled -o widget-archive.toml
```

The result is self-contained: no sliced file, no profile library, no
config. This is the format to archive a quote in, or to hand to someone
else.

`--retier` always needs `-o`.

## Choose the tier when creating

`-t/--type` saves at a tier directly, skipping the retier step:

```sh
printcalc widget.gcode -p mini -f pla-grey -m 12 -t bundled -o quote.toml
```

| Tier | Profiles | Sliced file | Use for |
|---|---|---|---|
| `default` | linked by name | re-read from path | live quotes |
| `full` | embedded | re-read from path | sharing without your library |
| `bundled` | embedded | embedded | archiving, sending elsewhere |

Tiers only go up. A bundled job cannot be turned back into a linked one —
the original names and paths are gone once resolved away. See
[job tiers](../explanation/job-tiers.md).

## Edit a saved job

Job files are plain TOML and meant to be edited. Common changes:

```toml
quantity = 25

[labor]
minutes = 6.0
rate_per_hour = 30.0

[[hardware]]
name = "magnet"
cost = 0.25
quantity = 2

margin_percentages_override = [45.0, 60.0]
```

Then re-cost. Every key is listed in
[the job file reference](../reference/job-file.md).

## Cost one plate of a multi-plate file

```sh
printcalc plates.gcode.3mf -P 2 -p mini -f pla-grey -m 5 -o plate2.toml
```

Passing `-P` to an existing job overrides its stored selection for that
run, and persists it only if you also pass `-o`.
