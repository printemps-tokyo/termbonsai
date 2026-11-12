# termbonsai

> A bonsai that lives in your terminal: it grows while you keep it watered, wilts when you forget, follows the seasons, and fits in a tmux status line. Zero-dependency Rust CLI.

[![CI](https://github.com/printemps-tokyo/termbonsai/actions/workflows/ci.yml/badge.svg)](https://github.com/printemps-tokyo/termbonsai/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

```console
$ termbonsai
                   &&&&&&&
                  &&&&&& &&&& &&&
                 &&&&&&& &&&&&&& &
                    &&\&&&&&&&&&&&&&&&&&&&&&&&
                 &&&&&&__\&& |  &&&&&&&&&&&&&&&
                &&&&&&&&&&\& |&&&&&&&&&&&_&&
                   ____\&&&__| /_______
                  &&&&&&\_&&&|_ /_
                     __\_____\ /
                        ____\ \
                             __/
                              |
                             /|\
                   :===================:
                    \_________________/

Keyaki, planted 60 days ago, 100% grown. Water 100%: growing. The soil dries out in about 3 days.
```

Most terminal trees are drawn once. This one is alive: it starts as a sprout,
grows a little every hour its soil is wet, and needs you to water it every
couple of days. Forget, and it stops growing, drops its leaves, and dies.
In spring it blossoms, in autumn it turns red and gold, in winter it thins
out. Each tree's shape comes from its seed, so no two are alike, and it is
drawn in 24-bit color where the terminal supports it.

## Install

From source, with a Rust toolchain 1.89 or later (https://rustup.rs):

```bash
cargo install --git https://github.com/printemps-tokyo/termbonsai
```

Tagged [releases](https://github.com/printemps-tokyo/termbonsai/releases) carry
prebuilt binaries for Linux (x86_64) and macOS (x86_64, Apple silicon), with
SHA-256 checksums. No dependencies beyond the Rust standard library.

## Usage

```bash
termbonsai                 # look at it (the first time, a tree is planted)
termbonsai water           # water it
termbonsai grow            # watch it grow from a sprout to today
termbonsai line            # one line for a status bar
termbonsai status          # its state as JSON
termbonsai plant --force   # start over with a new tree
termbonsai --at +30d --watered   # how it will look in 30 days, if you keep watering
termbonsai prune           # cut it back to a tidier shape (once a day)
termbonsai plant --pot pine --species pine   # a second pot, a black pine
termbonsai pots            # every pot at a glance
termbonsai water --all     # water them all
```

| Command | What it does |
| --- | --- |
| `show` (default) | Draw the tree and how it is doing. The first run plants one. |
| `water` | Water it. Watering soil that is still moist (90% or more) does nothing. `--all` waters every pot. |
| `plant` | Plant a new tree; `--seed N` for a given shape, `--name NAME`, `--species S`; `--force` replaces a living tree. |
| `prune` | Cut the canopy back by a column on each side and a row on top (see Pruning). |
| `pots` | One line for every pot: its name, the tree, its age, water and health. |
| `grow` | Replay its growth from a sprout to today (an animation on a terminal; it clears the screen). |
| `line` | One line, like `Momiji 12d water 64% ok`; `--tmux` adds tmux colors. |
| `status` | Its state as JSON. |

| Option | Description | Default |
| --- | --- | --- |
| `--at OFFSET` | Show it at a later time, like `+30d` or `+12h`; nothing is saved (the tree keeps no history, so it cannot look back) | |
| `--watered` | With `--at`: as if watered all along | |
| `--season S` | `auto`, `spring`, `summer`, `autumn` or `winter` | `auto` (the month) |
| `--south` | Southern hemisphere seasons | |
| `--width N` | Picture width, 24 to 200 | `60` |
| `--color C` | `auto`, `always` or `never`; `auto` honors [`NO_COLOR`](https://no-color.org/) and is off when not a terminal | `auto` |
| `--lang L` | `auto`, `ja` or `en`; `auto` reads `LC_ALL`, `LC_MESSAGES`, `LANG` | `auto` |
| `--species S` | With `plant`: `classic`, `pine`, `maple`, `cherry` or `random` (by the seed) | `classic` |
| `--pot NAME` | Another pot, kept beside the first (1 to 32 letters, digits, `-`, `_`) | the first pot |
| `--all` | With `water`: every pot | |
| `--state PATH` | The state file | see below |

## How it lives

- A full watering keeps the soil wet for 3 days; the soil dries steadily.
- It grows only while the soil is wet: one day of growth per day of wet soil.
  A tree fills out after about a month of wet soil (27 to 46 days, depending on
  its shape); forgotten days do not count.
- Below 25% water it is thirsty. Dry for 4 days, it starts to drop its leaves;
  dry for 10 days, it dies. Watering a dry or wilting tree brings it back with
  all its growth.
- The season follows the current month (in UTC; `--south` for the southern
  hemisphere): blossoms in spring (March to May), green in summer, red, orange
  and gold in autumn, sparse in winter.
- Nothing runs in the background. The state is a small text file with the time
  it was last watered; every look works out the present from it, so the tree
  keeps living while the terminal is closed.

## Species

| Species | Shape | Through the year |
| --- | --- | --- |
| `classic` | The tree termbonsai has always grown | Blossoms in spring, green in summer, red, orange and gold in autumn, sparse in winter |
| `pine` | A taller, bending trunk, long flat branches, thin pads of needles (`^`) | Evergreen: new light candles in spring, a touch of bronze in autumn, dark green all winter |
| `maple` | Classic proportions, fuller pads | Red new leaves in spring, deep reds in autumn, bare in winter |
| `cherry` | Wide branches that rise, a bigger crown | Covered in blossom (`*`) in spring, orange and red in autumn, bare in winter |

A classic tree grows exactly as before species existed, and its state file has
no species line. The species is fixed at planting.

## Pruning

`termbonsai prune` cuts the canopy back to one column narrower on each side
and one row lower than its widest branch and its top, never tighter than 3
columns either side of the trunk by 4 rows. The bounds stay: the tree keeps
growing inside them, and the next pruning tightens them further. Only a
healthy tree is pruned (not a dry, wilting or dead one), at most once in 24
hours; a young tree with nothing past the bounds has nothing to cut. `prune`
exits 1 when it cut nothing.

## Pots

`--pot NAME` gives any command another tree, kept in its own state file next
to the first one (`bonsai@NAME` beside `bonsai`), with its own lock. Every
command works on one pot; `termbonsai pots` lists them all, and
`termbonsai water --all` waters each one in turn (a pot that cannot be read is
reported, the others are still watered, and the run exits 2). Only plain files
count as pots; links and directories named like one are skipped:

```console
$ termbonsai pots
(first)      Keyaki 40d water 64% ok
cherry       Sakura 12d water 20% thirsty
pine         Matsu 25d water 67% ok
```

## In a tmux status bar

```tmux
set -g status-interval 60
set -g status-right "#(termbonsai line --tmux)"
```

`--tmux` colors the line green while it is fine, yellow when it needs water
and red when it is wilting or dead (tmux interprets the `#[fg=...]` codes in
`#()` output; checked with tmux 3.6a).

## The state file

`--state PATH`, or `TERMBONSAI_STATE`, or `$XDG_STATE_HOME/termbonsai/bonsai`,
or `~/.local/state/termbonsai/bonsai`. It is plain text (`key=value` lines).
A save writes a new temporary file and renames it into place, and an
operating-system lock on a file next to it (`bonsai.lock`) keeps two termbonsai
from changing the tree at once; the lock ends with the process, even a crashed
one. A file that
cannot be read is reported; `termbonsai plant --force` starts over. To keep a
tree on another machine, copy the file. Other pots are the same kind of file
with `@NAME` added to the name. Version 0.1 reads a 0.2 file as a classic,
unpruned tree (it ignores the keys it does not know).

## Credits

Inspired by terminal tree programs like [cbonsai](https://gitlab.com/jallbrit/cbonsai).
termbonsai is a separate program written from scratch: none of their code or
art is used.

## License

[MIT](./LICENSE) (c) printemps.tokyo
