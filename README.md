# formatui

Rename files using ordered rules. It has an interactive TUI and a plain CLI for
scripts.

```
Example.Show.2024.S02E06.1080p.HEVC.x265-GROUP[site.to].mkv  →  Example Show - S02E06.mkv
Sample.Movie.1999.1080p.BluRay.avi                                →  Sample Movie (1999).avi
```

![formatui demo](assets/demo.gif)

The `example/` folder holds empty sample files to try it on: `formatui example`.

## Installation

### macOS (Homebrew)

Homebrew on macOS (Apple Silicon and Intel) is the supported way to install, with prebuilt binaries:

```sh
brew tap simoncoulton/formatui https://github.com/simoncoulton/formatui
brew install formatui
```

### Arch Linux and other distributions

There is no package for Arch or its derivatives yet. Build from source with a recent
[Rust toolchain](https://rustup.rs) (1.89 or newer):

```sh
git clone https://github.com/simoncoulton/formatui
cd formatui
cargo install --path .
```


## Quick start

```sh
formatui                  # TUI on the current directory
formatui ~/Downloads      # TUI on another directory
formatui run . --apply    # no UI, rename everything that matches
```

On first run a default config is created at `~/.config/formatui/config.yml`
with a `tv` and a `movie` rule.

## TUI

A bordered header shows `formatui`, the directory, and how many files are
selected (plus conflict and unmatched counts). Below it the file list shows
`old → new [rule]`. Green is a pending rename, yellow is a conflict, grey is
unmatched.

| Key               | Action                                          |
| ----------------- | ----------------------------------------------- |
| `j` / `k`, arrows | Move                                            |
| `g` / `G`         | First / last file (`Home` / `End` too)          |
| `PgUp` / `PgDn`   | Move 10 rows                                    |
| `Space`           | Select / deselect the current file for renaming |
| `A` / `N`         | Select all / none                               |
| `a`               | Apply the selected renames (`y` confirms, any other key cancels) |
| `u`               | Undo the last applied batch                     |
| `r`               | Open the rules modal                            |
| `t`               | Show / hide files no rule matches               |
| `T`               | Theme picker: live preview, `b` toggles the transparent background |
| `F5`              | Rescan the directory                            |
| `s`               | Save rule changes to the config file            |
| `E`               | Open the config file in `$VISUAL` / `$EDITOR` (default `vi`) and reload it when the editor exits |
| `?` / `F1`        | Help: every key, placeholders, filters, steps (scrolls) |
| `q`               | Quit (warns about unsaved rule changes)         |

The footer only shows the everyday keys and drops the least important ones when
the terminal is narrow; everything else is in the help popup (`?`). Status
messages appear in the footer for a few seconds, green for success and red for
errors.

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
| `d`                      | Delete rule (asks first)            |
| `J` / `K`                | Move rule down / up                 |
| `s`                      | Save                                |
| `E`                      | Open the config file in your editor |
| `?` (`F1` while editing) | Show available placeholders         |
| `Esc`                    | Close the modal                     |

The preview and the file list update on every keystroke. Invalid regexes,
unknown groups, filters or steps are shown in red. While a text field has the
keyboard every printable key is text, including `q` and `?`; use `F1` for help.

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

`~/.config/formatui/config.yml`. Rules are tried in order and the **first match
wins**.

```yaml
show_unmatched: true # TUI: list files no rule matches (toggle with t, remembered)
theme: default # a built-in, a file in themes/, or one under themes:
transparent_background: true # use the terminal's background instead of the theme's

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
- The TUI edits the file in place: saving rules replaces only the `rule:` block,
  and the theme and `t` toggle replace only their own line. Your comments and
  other settings are kept. Comments inside the `rule:` block are not.
- Unknown keys are errors, reported with the file, line and column. A file that
  doesn't parse is never overwritten.

## Themes

Pick one with `T` (live preview, `Enter` saves it as `theme:` in the config).
Built-ins: `default` (your terminal's own colours), `tokyo-night`,
`catppuccin-frappe`, `catppuccin-mocha`, `catppuccin-macchiato`,
`catppuccin-latte`, `dracula`, `one-dark`, `gruvbox-dark`, `nord`,
`solarized-dark`, `rose-pine`, `monokai`, `github-dark`, `everforest` and
`kanagawa`. The RGB themes need a true-colour terminal.

Themes are plain `.yml` files in `~/.config/formatui/themes/`, read each time you
open the picker or reload the config, so there is nothing to rebuild:

- On start, every built-in without a file there gets one with every colour
  spelled out. Edit it, or delete it to get the original back.
- Any other `.yml` file in that folder is a new theme named after the file.
- `base: <theme>` fills in any colour you leave out. Colours are `#rrggbb`, an
  ANSI name (`red`, `darkgray`, `reset`) or a 0-255 index.
- A `themes:` map in `config.yml` can define or tweak themes too, and wins over
  the files.

```yaml
# ~/.config/formatui/themes/my-night.yml
base: nord
accent: "#ff8800"
```

`transparent_background: true` (the default) lets your terminal's background
show instead of the theme's page colour; toggle it with `b` in the picker.
Bad colours, unknown bases or unknown keys are reported with the theme and file.

## Safety

- Existing files are never overwritten, and when two files map to the same name
  only the first is renamed; the other is flagged as a conflict.
- Hidden files are ignored.
- `/` is replaced with `-` in generated names.
- Undo skips any file that has since moved or whose original name has been
  taken.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

See [AGENTS.md](AGENTS.md) for how the code is organised and the UI conventions.
