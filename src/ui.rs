//! Rendering only: `draw(frame, &app)` never mutates state, and every colour
//! comes from `app.theme`.
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use regex::Regex;

use crate::app::{App, Confirm, FIELDS, Mode};
use crate::plan::{Entry, Status};
use crate::theme::Theme;

pub fn draw(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let full = f.area();
    f.render_widget(Block::new().style(t.base()), full);
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(full);

    draw_header(f, app, header);
    draw_files(f, app, main);
    draw_footer(f, app, footer);

    // Popups are drawn last, centred over everything.
    match app.mode {
        Mode::Normal => {}
        Mode::Rules | Mode::Edit => draw_rules(f, app, main),
        Mode::Confirm(what) => draw_confirm(f, app, what, centered(full, 46, 7)),
        Mode::Themes => draw_themes(f, app, full),
    }
    if app.help {
        draw_help(
            f,
            app,
            centered(full, pct(full.width, 86), pct(full.height, 94)),
        );
    }
}

// ---- helpers -----------------------------------------------------------------------------

fn pct(v: u16, p: u32) -> u16 {
    (u32::from(v) * p / 100) as u16
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let (w, h) = (w.min(area.width), h.min(area.height));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Clears the area and applies the theme's base style, so the background is right
/// in both opaque and transparent modes.
fn popup(f: &mut Frame, t: &Theme, area: Rect) {
    f.render_widget(Clear, area);
    f.render_widget(Block::new().style(t.base()), area);
}

fn title<'a>(t: &Theme, text: &str) -> Span<'a> {
    Span::styled(
        text.to_string(),
        Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
    )
}

fn key<'a>(t: &Theme, k: &str) -> Span<'a> {
    Span::styled(
        k.to_string(),
        Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
    )
}

fn muted<'a>(t: &Theme, s: impl Into<String>) -> Span<'a> {
    Span::styled(s.into(), Style::new().fg(t.muted))
}

/// `"Enter save  Esc cancel"`: items are separated by two spaces and the first
/// word of each is the key.
fn hint_line<'a>(t: &Theme, hints: &str) -> Line<'a> {
    let mut spans = vec![muted(t, " ")];
    for (i, item) in hints.split("  ").enumerate() {
        if i > 0 {
            spans.push(muted(t, "  "));
        }
        let (k, rest) = item.split_once(' ').unwrap_or((item, ""));
        spans.push(key(t, k));
        if !rest.is_empty() {
            spans.push(muted(t, format!(" {rest}")));
        }
    }
    spans.push(muted(t, " "));
    Line::from(spans)
}

/// Every pane and popup uses softly rounded corners.
fn rounded<'a>() -> Block<'a> {
    Block::bordered().border_type(BorderType::Rounded)
}

fn panel<'a>(t: &Theme, name: &str) -> Block<'a> {
    rounded()
        .border_style(Style::new().fg(t.border))
        .title(title(t, name))
}

fn modal<'a>(t: &Theme, name: &str, hints: &str) -> Block<'a> {
    rounded()
        .border_style(Style::new().fg(t.accent))
        .title(title(t, name))
        .title_bottom(hint_line(t, hints))
}

/// Selection only: ratatui keeps the selected row in view, so the list scrolls with it.
fn list_state(sel: Option<usize>) -> ListState {
    ListState::default().with_selected(sel)
}

// ---- main screen -------------------------------------------------------------------------

fn entry_line(app: &App, e: &Entry, checkbox: bool) -> Line<'static> {
    let t = &app.theme;
    let name = |p: &std::path::Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string()
    };
    let old = name(&e.path);
    let (arrow, new, color) = match (&e.status, &e.target) {
        (Status::Ready, Some(p)) => (" → ", name(p), t.success),
        (Status::Conflict(why), Some(p)) => {
            (" ✗ ", format!("{} (conflict: {why})", name(p)), t.warning)
        }
        (Status::Unchanged, _) => (" ", "(unchanged)".into(), t.muted),
        _ => ("", String::new(), t.muted),
    };
    let rule = e
        .rule
        .and_then(|i| app.rules.get(i))
        .map(|r| format!("  [{}]", r.name))
        .unwrap_or_default();
    let boxed = match (checkbox, e.status == Status::Ready) {
        (true, true) if app.excluded.contains(&e.path) => {
            Span::styled("[ ] ", Style::new().fg(t.muted))
        }
        (true, true) => Span::styled("[x] ", Style::new().fg(t.fg)),
        (true, false) => Span::raw("    "),
        _ => Span::raw(""),
    };
    let old_color = if e.status == Status::NoMatch {
        t.muted
    } else {
        t.fg
    };
    Line::from(vec![
        boxed,
        Span::styled(old, Style::new().fg(old_color)),
        Span::raw(arrow),
        Span::styled(new, Style::new().fg(color)),
        muted(t, rule),
    ])
}

/// The app's name, with the directory and what is selected: global facts only.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let block = panel(t, " formatui ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let count = |want: fn(&Status) -> bool| app.entries.iter().filter(|e| want(&e.status)).count();
    let conflicts = count(|s| matches!(s, Status::Conflict(_)));
    let unmatched = count(|s| *s == Status::NoMatch);

    let mut summary = vec![
        Span::styled(
            app.selected_count().to_string(),
            Style::new().fg(t.success).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" of {} selected", app.ready_count()),
            Style::new().fg(t.fg),
        ),
    ];
    if conflicts > 0 {
        summary.push(Span::styled(
            format!(
                "   {conflicts} conflict{}",
                if conflicts == 1 { "" } else { "s" }
            ),
            Style::new().fg(t.warning),
        ));
    }
    if unmatched > 0 {
        summary.push(muted(t, format!("   {unmatched} unmatched")));
    }
    let summary = Line::from(summary);
    let width = (summary.width() as u16).min(inner.width);
    let [dir, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(inner);
    f.render_widget(
        Paragraph::new(Line::styled(
            format!("{}", app.dir.display()),
            Style::new().fg(t.fg),
        )),
        dir,
    );
    f.render_widget(Paragraph::new(summary), right);
}

fn draw_files(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let items: Vec<ListItem> = app
        .visible()
        .iter()
        .map(|e| ListItem::new(entry_line(app, e, true)))
        .collect();
    let list = List::new(items)
        .block(panel(t, " Files "))
        .highlight_style(t.selection());
    f.render_stateful_widget(list, area, &mut list_state(app.file_sel));
}

struct Hint {
    key: &'static str,
    desc: &'static str,
    /// 0 is never dropped; otherwise the highest rank (rightmost first) goes first.
    rank: u8,
}

fn hint_width(h: &Hint) -> usize {
    h.key.chars().count() + 1 + h.desc.chars().count()
}

fn footer_hints(app: &App, width: usize) -> Vec<Hint> {
    let h = |key, desc, rank| Hint { key, desc, rank };
    let has_files = !app.visible().is_empty();
    let mut hints = Vec::new();
    if has_files {
        hints.push(h("space", "select", 2));
    }
    hints.push(h("a", "apply", 0));
    hints.push(h("u", "undo", 3));
    hints.push(h("r", "rules", 1));
    hints.push(h(
        "t",
        if app.show_all {
            "hide unmatched"
        } else {
            "show unmatched"
        },
        4,
    ));
    hints.push(h("?", "help", 0));
    hints.push(h("q", "quit", 0));

    let total =
        |v: &[Hint]| v.iter().map(hint_width).sum::<usize>() + 2 * v.len().saturating_sub(1);
    while total(&hints) > width {
        let Some(max) = hints.iter().map(|h| h.rank).max().filter(|r| *r > 0) else {
            break;
        };
        let at = hints.iter().rposition(|h| h.rank == max).unwrap_or(0);
        hints.remove(at);
    }
    hints
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let line = if let Some(s) = &app.status {
        let color = if s.error { t.error } else { t.success };
        Line::styled(s.text.clone(), Style::new().fg(color))
    } else if app.mode == Mode::Normal && !app.help {
        let mut spans = Vec::new();
        for (i, h) in footer_hints(app, usize::from(area.width))
            .iter()
            .enumerate()
        {
            if i > 0 {
                spans.push(muted(t, "  "));
            }
            spans.push(key(t, h.key));
            spans.push(muted(t, format!(" {}", h.desc)));
        }
        Line::from(spans)
    } else {
        Line::raw("")
    };
    f.render_widget(Paragraph::new(line), area);
}

// ---- rules modal -------------------------------------------------------------------------

fn draw_rules(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    popup(f, t, area);
    let editing = app.mode == Mode::Edit;
    let hints = if editing {
        "Tab next field  F1 help  Esc done"
    } else {
        "j/k select  e edit  n new  d delete  J/K reorder  s save  E config  ? help  Esc close"
    };
    let heading = if app.dirty {
        " Rules (modified, s to save) "
    } else {
        " Rules "
    };
    let outer = modal(t, heading, hints);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let [top, preview_area] =
        Layout::vertical([Constraint::Length(9), Constraint::Min(0)]).areas(inner);
    let [list_area, editor_area] =
        Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(72)]).areas(top);

    let rule_items: Vec<ListItem> = app
        .rules
        .iter()
        .zip(&app.compiled)
        .enumerate()
        .map(|(i, (r, c))| {
            let color = if c.is_err() { t.error } else { t.fg };
            ListItem::new(format!("{}. {}", i + 1, r.name)).style(Style::new().fg(color))
        })
        .collect();
    let rules = List::new(rule_items)
        .block(panel(t, " Order (first match wins) "))
        .highlight_style(t.selection());
    f.render_stateful_widget(rules, list_area, &mut list_state(app.rule_sel));

    let mut lines = Vec::new();
    if let Some(i) = app.rule_sel.filter(|i| *i < app.rules.len()) {
        let rule = &app.rules[i];
        let (steps, exts) = if editing {
            (app.steps_buf.clone(), app.exts_buf.clone())
        } else {
            (rule.steps.join(", "), rule.extensions.join(", "))
        };
        let values = [
            rule.name.clone(),
            rule.pattern.clone(),
            rule.template.clone(),
            steps,
            exts,
        ];
        for (n, label) in FIELDS.iter().enumerate() {
            let active = editing && app.field == n;
            let style = if active {
                Style::new().fg(t.accent)
            } else {
                Style::new().fg(t.fg)
            };
            let cursor = if active { "▏" } else { "" };
            lines.push(Line::from(vec![
                muted(t, format!("{label:<9}")),
                Span::styled(format!("{}{cursor}", values[n]), style),
            ]));
        }
        if let Some(Err(e)) = app.compiled.get(i) {
            lines.push(Line::styled(e.clone(), Style::new().fg(t.error)));
        }
    } else {
        lines.push(Line::from(vec![
            muted(t, "No rules. Press "),
            key(t, "n"),
            muted(t, " to create one."),
        ]));
    }
    let name = if editing { " Edit " } else { " Rule " };
    f.render_widget(Paragraph::new(lines).block(panel(t, name)), editor_area);

    let matched: Vec<ListItem> = app
        .entries
        .iter()
        .filter(|e| app.rule_sel.is_some() && e.rule == app.rule_sel)
        .map(|e| ListItem::new(entry_line(app, e, false)))
        .collect();
    let heading = format!(" Preview: {} file(s) matched by this rule ", matched.len());
    f.render_widget(List::new(matched).block(panel(t, &heading)), preview_area);
}

// ---- small dialogs -----------------------------------------------------------------------

fn draw_confirm(f: &mut Frame, app: &App, what: Confirm, area: Rect) {
    let t = &app.theme;
    popup(f, t, area);
    let (name, question, detail) = match what {
        Confirm::Apply => {
            let left = app.ready_count() - app.selected_count();
            let detail = if left > 0 {
                format!("{left} deselected file(s) will be left alone")
            } else {
                String::new()
            };
            (
                " Apply ",
                format!("Rename {} file(s)?", app.selected_count()),
                detail,
            )
        }
        Confirm::DeleteRule => {
            let rule = app.rule_sel.and_then(|i| app.rules.get(i));
            (
                " Delete rule ",
                format!("Delete rule '{}'?", rule.map_or("", |r| r.name.as_str())),
                "Rules are written to the file when you save (s)".to_string(),
            )
        }
    };
    let block = modal(t, name, "y yes  Esc cancel").border_style(Style::new().fg(t.warning));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::raw(""),
        Line::styled(question, Style::new().fg(t.fg)).centered(),
        Line::styled(detail, Style::new().fg(t.muted)).centered(),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_themes(f: &mut Frame, app: &App, full: Rect) {
    let t = &app.theme;
    let height = (app.themes.len() as u16 + 3).clamp(4, full.height.saturating_sub(2).max(4));
    let area = centered(full, full.width.saturating_sub(4).clamp(1, 56), height);
    popup(f, t, area);
    let block = modal(
        t,
        " Theme ",
        "↑↓ preview  b background  Enter save  Esc cancel",
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [bg_row, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    let bg = if app.transparent {
        "transparent (terminal's own)"
    } else {
        "theme colour"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            muted(t, "Background  "),
            Span::styled(bg, Style::new().fg(t.accent)),
        ])),
        bg_row,
    );

    let name_w = app
        .themes
        .iter()
        .map(|th| th.name.chars().count())
        .max()
        .unwrap_or(0);
    let items: Vec<ListItem> = app
        .themes
        .iter()
        .map(|th| {
            let mut spans = vec![Span::raw(format!("{:<name_w$}  ", th.name))];
            // Each theme's own colours, so you can compare before choosing.
            spans.extend(
                th.swatch()
                    .into_iter()
                    .map(|c| Span::styled("██ ", Style::new().fg(c))),
            );
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(t.selection())
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, list_area, &mut list_state(Some(app.theme_idx)));
}

// ---- help --------------------------------------------------------------------------------

fn help_lines<'a>(app: &App) -> Vec<Line<'a>> {
    let t = &app.theme;
    let head = |s: &str| Line::from(title(t, s));
    let row = |k: &str, d: &str| {
        Line::from(vec![
            Span::raw("  "),
            key(t, &format!("{k:<16}")),
            Span::styled(d.to_string(), Style::new().fg(t.fg)),
        ])
    };
    let code = |s: &str| {
        Span::styled(
            s.to_string(),
            Style::new().fg(t.fg).add_modifier(Modifier::BOLD),
        )
    };

    let groups: Vec<String> = app
        .rule_sel
        .and_then(|i| app.rules.get(i))
        .and_then(|r| Regex::new(&r.pattern).ok())
        .map(|re| {
            re.capture_names()
                .flatten()
                .map(|g| format!("{{{g}}}"))
                .collect()
        })
        .unwrap_or_default();
    let groups_line = if groups.is_empty() {
        muted(
            t,
            "none yet: add named groups like (?P<title>.+) to the pattern",
        )
    } else {
        code(&groups.join("  "))
    };

    vec![
        head("Files"),
        row(
            "j/k  g/G",
            "move, first / last   (PgUp PgDn Home End also work)",
        ),
        row("space", "select / deselect the file for renaming"),
        row("A  N", "select all / none"),
        row("a", "apply the selected renames (asks first)"),
        row("u", "undo the last applied batch"),
        row("t", "hide / show files no rule matches"),
        row("F5", "rescan the directory"),
        row("r", "rules: add, edit, reorder and preview"),
        row("T", "theme picker (b toggles the transparent background)"),
        row(
            "E",
            "open the config file in $VISUAL / $EDITOR, reload on exit",
        ),
        row("s", "save rule changes"),
        row("q", "quit"),
        Line::raw(""),
        head("Placeholders (Template field)"),
        row("{name}", "value of the group (?P<name>…) in the Pattern"),
        row("{name|filter}", "transform the value; chain with more |"),
        Line::from(vec![
            Span::raw("  Groups in the selected rule: "),
            groups_line,
        ]),
        Line::raw(""),
        head("Filters"),
        row("upper lower", "change case"),
        row("title trim", "Title Case each word / strip spaces"),
        row("pad2", "zero-pad to 2 digits (2 → 02)"),
        Line::raw(""),
        head("Steps (cleanup before the pattern runs, in order)"),
        row("strip_brackets", "remove [anything in square brackets]"),
        row("dots_to_spaces", "Example.Show → Example Show"),
        row("underscores_to_spaces", ""),
        row("collapse_spaces", "squeeze repeated whitespace"),
        Line::raw(""),
        head("Formats"),
        Line::raw("  Comma separated extensions the rule applies to (mkv, mp4, avi). Empty = any."),
        Line::raw(""),
        head("Example"),
        Line::from(vec![
            Span::raw("  Pattern   "),
            code(r"^(?P<title>.+?) S(?P<season>\d+)E(?P<episode>\d+)"),
        ]),
        Line::from(vec![
            Span::raw("  Template  "),
            code("{title} - S{season|pad2}E{episode|pad2}"),
        ]),
        Line::raw("  Example Show S2E6  →  Example Show - S02E06"),
    ]
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    popup(f, t, area);
    let block = modal(t, " Help ", "j/k scroll  Esc close").border_style(Style::new().fg(t.accent));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = help_lines(app);
    let max = lines.len().saturating_sub(usize::from(inner.height));
    let scroll = usize::from(app.help_scroll).min(max) as u16;
    f.render_widget(
        Paragraph::new(lines)
            .style(t.base())
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use super::*;
    use crate::config::Config;

    fn app_with(theme: &str, transparent: bool) -> App {
        let cfg = Config {
            theme: theme.into(),
            transparent_background: transparent,
            ..Config::default()
        };
        let themes = crate::theme::all_themes(&Default::default()).unwrap();
        let mut app = App::new(cfg, PathBuf::from("d"), themes).unwrap();
        let files = [
            "d/Show.2024.S01E01.1080p.mkv",
            "d/Some.Film.1999.1080p.mp4",
            "d/notes.txt",
        ];
        app.set_files(files.iter().map(PathBuf::from).collect(), HashSet::new());
        app
    }

    fn render(app: &App, w: u16, h: u16) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        term.backend().buffer().clone()
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    /// Column of `needle` in a rendered row (rows contain multibyte border characters).
    fn col(row: &str, needle: &str) -> u16 {
        row[..row.find(needle).unwrap()].chars().count() as u16
    }

    fn text(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| row(buf, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lists_files_and_footer_hints() {
        let buf = render(&app_with("default", true), 100, 12);
        let all = text(&buf);
        assert!(
            all.contains("Show - S01E01.mkv") && all.contains("Some Film (1999).mp4"),
            "{all}"
        );
        let footer = row(&buf, 11);
        for want in [
            "space select",
            "a apply",
            "r rules",
            "hide unmatched",
            "? help",
            "q quit",
        ] {
            assert!(footer.contains(want), "{footer}");
        }
        assert!(
            !footer.contains("PgUp") && !footer.contains("j/k"),
            "{footer}"
        );
    }

    #[test]
    fn header_names_the_app_with_directory_and_selection() {
        let mut app = app_with("catppuccin-mocha", false);
        let buf = render(&app, 80, 10);
        assert!(row(&buf, 0).contains("formatui"), "{}", row(&buf, 0));
        let line = row(&buf, 1);
        assert!(
            line.contains('d') && line.contains("2 of 2 selected") && line.contains("1 unmatched"),
            "{line}"
        );
        assert!(row(&buf, 3).contains("Files"));
        // The title is the accent colour.
        let x = col(&row(&buf, 0), "formatui");
        assert_eq!(buf[(x, 0)].fg, app.theme.accent);
        // Unticking a file shows up in the header.
        app.excluded
            .insert(PathBuf::from("d/Show.2024.S01E01.1080p.mkv"));
        assert!(row(&render(&app, 80, 10), 1).contains("1 of 2 selected"));
    }

    #[test]
    fn footer_toggle_text_follows_the_setting() {
        let mut app = app_with("default", true);
        app.show_all = false;
        assert!(row(&render(&app, 100, 6), 5).contains("show unmatched"));
    }

    #[test]
    fn footer_keys_use_the_accent_colour() {
        let app = app_with("catppuccin-mocha", false);
        let buf = render(&app, 100, 8);
        let accent = app.theme.accent;
        let keys: String = (0..100u16)
            .map(|x| &buf[(x, 7)])
            .filter(|c| c.fg == accent)
            .map(|c| c.symbol().to_string())
            .collect();
        assert!(
            keys.contains('a') && keys.contains('q') && keys.contains('?'),
            "{keys}"
        );
    }

    #[test]
    fn narrow_footer_drops_low_ranked_hints_but_keeps_apply_help_quit() {
        let buf = render(&app_with("default", true), 28, 6);
        let footer = row(&buf, 5);
        assert!(
            footer.contains("apply") && footer.contains("help") && footer.contains("quit"),
            "{footer}"
        );
        assert!(
            !footer.contains("undo") && !footer.contains("unmatched"),
            "{footer}"
        );
    }

    #[test]
    fn selection_dependent_hints_hide_with_no_files() {
        let mut app = app_with("default", true);
        app.set_files(vec![], HashSet::new());
        assert!(!row(&render(&app, 100, 6), 5).contains("space"));
    }

    #[test]
    fn status_messages_are_green_or_red() {
        let mut app = app_with("catppuccin-mocha", false);
        app.ok("saved");
        let buf = render(&app, 40, 5);
        assert_eq!(buf[(0, 4)].fg, app.theme.success);
        app.fail("boom");
        assert_eq!(render(&app, 40, 5)[(0, 4)].fg, app.theme.error);
    }

    fn every_view(app: &mut App) -> Vec<(&'static str, Mode, bool)> {
        let _ = app;
        vec![
            ("normal", Mode::Normal, false),
            ("rules", Mode::Rules, false),
            ("edit", Mode::Edit, false),
            ("apply", Mode::Confirm(Confirm::Apply), false),
            ("delete", Mode::Confirm(Confirm::DeleteRule), false),
            ("themes", Mode::Themes, false),
            ("help", Mode::Normal, true),
        ]
    }

    #[test]
    fn nothing_panics_at_any_size() {
        for theme in ["default", "nord"] {
            let mut app = app_with(theme, true);
            for (name, mode, help) in every_view(&mut app) {
                app.mode = mode;
                app.help = help;
                for (w, h) in [
                    (0, 0),
                    (1, 1),
                    (3, 2),
                    (10, 3),
                    (30, 8),
                    (60, 14),
                    (120, 40),
                ] {
                    let _ = (name, render(&app, w, h));
                }
            }
            app.set_files(vec![], HashSet::new());
            app.rules.clear();
            for (_, mode, help) in every_view(&mut app) {
                app.mode = mode;
                app.help = help;
                let _ = render(&app, 40, 10);
            }
        }
    }

    #[test]
    fn popups_keep_the_right_background() {
        for transparent in [false, true] {
            let mut app = app_with("catppuccin-mocha", transparent);
            let want = app.theme.bg;
            assert_eq!(want == ratatui::style::Color::Reset, transparent);
            for (name, mode, help) in every_view(&mut app) {
                app.mode = mode;
                app.help = help;
                let buf = render(&app, 100, 30);
                let mid = &buf[(50, 15)];
                assert_eq!(
                    mid.bg, want,
                    "{name} popup background (transparent={transparent})"
                );
            }
        }
    }

    #[test]
    fn modal_hints_use_the_accent_colour() {
        let mut app = app_with("catppuccin-mocha", false);
        app.mode = Mode::Confirm(Confirm::Apply);
        let buf = render(&app, 100, 30);
        let bottom = (0..30u16)
            .find(|y| row(&buf, *y).contains("y yes"))
            .expect("hint line");
        let x = col(&row(&buf, bottom), "y yes");
        assert_eq!(buf[(x, bottom)].fg, app.theme.accent);
    }

    #[test]
    fn rules_modal_shows_errors_inside_the_dialog() {
        let mut app = app_with("catppuccin-mocha", false);
        app.mode = Mode::Rules;
        app.rules[0].pattern = "(unclosed".into();
        app.set_files(vec![PathBuf::from("d/x.mkv")], HashSet::new());
        let buf = render(&app, 100, 30);
        let all = text(&buf);
        assert!(all.contains("pattern:"), "{all}");
        let y = (0..30u16)
            .find(|y| row(&buf, *y).contains("pattern:"))
            .unwrap();
        let x = col(&row(&buf, y), "pattern:");
        assert_eq!(buf[(x, y)].fg, app.theme.error);
    }

    #[test]
    fn help_scrolls_without_running_off_the_end() {
        let mut app = app_with("default", true);
        app.help = true;
        app.help_scroll = 500;
        let all = text(&render(&app, 100, 24));
        assert!(all.contains("Example Show S2E6"), "{all}");
    }
}
