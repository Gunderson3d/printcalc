# Job tiers

A saved job can hold references to things, or the things themselves.
Which one you want depends on whether the job is a live quote or a
record.

## The two independent choices

A job file has two links to the outside world, and each can be a
reference or embedded data.

**The sliced file** is either a path re-read at cost time, or a parsed
summary stored inline.

**The profiles** — printer, and one per material slot — are either names
resolved against your library at cost time, or full profile data stored
inline.

The three named tiers are the useful combinations:

| Tier | Profiles | Sliced file |
|---|---|---|
| `default` | linked by name | re-read from path |
| `full` | embedded | re-read from path |
| `bundled` | embedded | embedded |

There is no tier with embedded slice data but linked profiles. It has no
use: if you have gone to the trouble of freezing the expensive half, you
are archiving, and archiving with a live dependency on your profile
library defeats the purpose.

## What each tier is for

**`default`** is for a quote you are still working on. The job stays in
sync with reality: correct your printer's purchase price and every open
quote re-costs with the new number. That is exactly what you want while
you are still deciding, and exactly what you don't want after you have
sent a price to a customer.

**`full`** freezes the numbers but not the geometry. The job costs
identically on a machine with no profile library, which makes it the
right thing to hand to a colleague who doesn't share your profiles. It
still needs the sliced file.

**`bundled`** is self-contained: no sliced file, no profile library, no
config. This is the archive format. It is also the only tier the browser
build can cost on its own: with no filesystem there, a `default` or
`full` job has to be handed its sliced file alongside, and `ref =` names
resolve against the built-in presets only.

## Tiers only go up

Serialization can move a job from linked to embedded, and from a path to
an embedded summary. It cannot go the other way.

This is not a missing feature. A `Linked` reference holds a name; once it
is resolved into embedded profile data, the name is gone. A `Path` source
holds a location; once the file is parsed and the summary stored, the
location is gone. Neither is recoverable from what is left, and inventing
a plausible name would produce a job that silently points at the wrong
profile.

So `--retier` is one-way, and re-tiering writes to a new file rather than
editing in place:

```sh
printcalc widget.toml --retier bundled -o widget-archive.toml
```

Keep the `default`-tier job if you want to keep editing it.

## Why paths resolve against the job file

A relative `source.path` resolves against **the job file's own
directory**, not the working directory.

This means a job and its sliced file can be moved together, checked into
a repository together, or handed over as a pair, and the job keeps
working. Resolving against the working directory would make a job file's
meaning depend on where you happened to be standing when you ran the
command, which is not a property a saved record should have.

The cost is that a job saved outside the directory holding its sliced
file needs an absolute path — which is what the CLI writes when the two
aren't near each other.

## Choosing

A rule of thumb:

- Still negotiating? `default`.
- Sending it to someone? `full`.
- Filing it, or opening it in the browser without the sliced file to
  hand? `bundled`.

When in doubt, keep the `default`-tier job as the working copy and
produce a `bundled` copy at the moment you quote. The working copy stays
editable; the bundled copy is the record of what you actually said.

## See also

- [Save and reuse jobs](../how-to/save-and-reuse-jobs.md) — the commands.
- [Job files](../reference/job-file.md) — the TOML each tier produces.
