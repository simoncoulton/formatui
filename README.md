# formatui

Rename files using ordered rules. It has an interactive TUI and a plain CLI for
scripts.

```
Example.Show.2024.S02E06.1080p.HEVC.x265-GROUP[site.to].mkv  →  Example Show - S02E06.mkv
Sample.Movie.1999.1080p.BluRay.avi                                →  Sample Movie (1999).avi
```

![formatui demo](assets/demo.gif)

The `example/` folder holds empty sample files to try it on: `formatui example`.

## Install

```sh
cargo build --release
# binary: target/release/formatui
cargo install --path .   # optional, puts it on your PATH
```

## Quick start

```sh
formatui                  # TUI on the current directory
formatui ~/Downloads      # TUI on another directory
formatui run . --apply    # no UI, rename everything that matches
```

On first run a default config is created at `~/.config/formatui/config.yaml`
with a `tv` and a `movie` rule.

## TUI

The main screen lists files as `old → new [rule]`. Green is a pending rename,
yellow is a conflict, grey is unmatched.

| Key               | Action                                          |
| ----------------- | ----------------------------------------------- |
| `j` / `k`, arrows | Move                                            |
| `PgUp` / `PgDn`   | Move 10 rows                                    |
| `Home` / `End`    | First / last file                               |
| `Space`           | Select / deselect the current file for renaming |
| `A` / `N`         | Select all / none                               |
| `a`               | Apply the selected renames (asks `y/n`)         |
| `u`               | Undo the last applied batch                     |
| `r`               | Open the rules modal                            |
| `t`               | Show / hide files no rule matches               |
| `F5`              | Rescan the directory                            |
| `s`               | Save rules to the config file                   |
| `?` / `F1`        | Help: placeholders, filters, steps              |
| `q`               | Quit (warns about unsaved rule changes)         |

Every file with a pending rename starts selected (`[x]`). Untick the ones you
want to leave alone, then press `a`.

### Rules modal (`r`)

Rules are managed in a popup with the rule order, an editor, and a live preview
of the files the selected rule matches.

| Key                      | Action                              |
| ------------------------ | ----------------------------------- |
| `j` / `k`                | Select a rule                       |
| `e` / `Enter`            | Edit the selected rule              |
| `Tab` / `Shift-Tab`      | Next / previous field while editing |
| `Esc` / `Enter`          | Finish editing                      |
| `n`                      | New rule                            |
| `d`                      | Delete rule                         |
| `J` / `K`                | Move rule down / up                 |
| `s`                      | Save                                |
| `?` (`F1` while editing) | Show available placeholders         |
| `Esc`                    | Close the modal                     |

The preview and the file list update on every keystroke. Invalid regexes,
unknown groups, filters or steps are shown in red.

## CLI

```sh
formatui run <paths...> [-r] [-e mkv,mp4] [--apply]
formatui undo
formatui test "Some.Release.Name.S01E02.mkv"
formatui rules
```

- `run` is a **dry run** unless you pass `--apply`. Directories are not
  descended unless you pass `-r`, and `-e` restricts to extensions.
- `undo` reverts the most recent applied batch (TUI or CLI). History is kept in
  `~/.local/share/formatui/history.json`.
- `test` shows which rule matches a single name and what it produces.
- `--config <file>` (any command) uses a different config file.
- `--rules tv,movie` (or `--rules=tv`) limits `run`, `test` and `rules` to the
  named rules, for example `formatui --rules=tv run . --apply`. Names are
  case-insensitive, an unknown name is an error, and priority still follows the
  config order. Not supported in the TUI.

## Rules

`~/.config/formatui/config.yaml`. Rules are tried in order and the **first match
wins**.

```yaml
show_unmatched: true # TUI: list files no rule matches (toggle with t, remembered)

rule:
  - name: tv
    extensions: [mkv, mp4, avi]
    steps: [strip_brackets, dots_to_spaces, underscores_to_spaces]
    pattern: '(?i)^(?P<title>.+?)(?: (?:19|20)\d{2})?(?: -)? S(?P<season>\d+)E(?P<episode>\d+)'
    template: "{title} - S{season|pad2}E{episode|pad2}"
```

| Field        | Meaning                                                                     |
| ------------ | --------------------------------------------------------------------------- |
| `name`       | Label shown in the UI                                                       |
| `extensions` | File types the rule applies to. Defaults to `mkv, mp4, avi`; `[]` means any |
| `steps`      | Cleanup applied to the file name (without extension) before matching        |
| `pattern`    | [Rust regex](https://docs.rs/regex) with named groups `(?P<name>…)`         |
| `template`   | Output name; the extension is added back automatically                      |

**Placeholders.** `{name}` inserts the value of the named group. Add filters
with `|`, for example `{title|title}` or `{season|pad2}`.

**Filters:** `upper`, `lower`, `title`, `trim`, `pad2` (zero-pad to 2 digits).

**Steps:** `strip_brackets`, `dots_to_spaces`, `underscores_to_spaces`,
`collapse_spaces`.

Notes:

- Every `{group}` in the template must exist in the pattern.
- `dots_to_spaces` also affects dots inside titles (e.g. "St. Example"); use a
  custom rule for those.
- Saving from the TUI rewrites the file and drops comments.

## Safety

- Existing files are never overwritten, and when two files map to the same name
  only the first is renamed; the other is flagged as a conflict.
- Hidden files are ignored.
- `/` is replaced with `-` in generated names.
- Undo skips any file that has since moved or whose original name has been
  taken.

## Development

```sh
cargo test
```

The demo GIF is recorded from `example/` with
[vhs](https://github.com/charmbracelet/vhs) (also needs `ttyd` and `ffmpeg`):

```sh
docs/make-demo.sh   # rebuilds docs/demo.gif using a throwaway HOME, so your config and history are untouched
```

Edit `docs/demo.tape` to change the walkthrough.
