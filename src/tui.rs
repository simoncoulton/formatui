use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use regex::Regex;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::config::{self, Config, Rule};
use crate::engine::Compiled;
use crate::history;
use crate::plan::{self, Entry, Status};

#[derive(PartialEq)]
enum Mode {
    Normal,
    /// Rules modal, browsing the rule list.
    Rules,
    /// Rules modal, typing into a field.
    Edit,
    Confirm,
}

const FIELDS: [&str; 5] = ["Name", "Pattern", "Template", "Steps", "Formats"];
const PAGE: isize = 10;

struct App {
    dir: PathBuf,
    config_path: PathBuf,
    rules: Vec<Rule>,
    compiled: Vec<Result<Compiled, String>>,
    files: Vec<PathBuf>,
    entries: Vec<Entry>,
    /// Ready files the user unticked; everything else that is ready gets renamed.
    excluded: HashSet<PathBuf>,
    file_state: ListState,
    rule_state: ListState,
    mode: Mode,
    field: usize,
    steps_buf: String,
    exts_buf: String,
    dirty: bool,
    quit_armed: bool,
    show_all: bool,
    help: bool,
    msg: String,
    quit: bool,
}

pub fn run(cfg: Config, config_path: PathBuf, dir: PathBuf) -> Result<()> {
    let mut app = App {
        dir,
        config_path,
        rules: cfg.rules,
        compiled: vec![],
        files: vec![],
        entries: vec![],
        excluded: HashSet::new(),
        file_state: ListState::default(),
        rule_state: ListState::default(),
        mode: Mode::Normal,
        field: 0,
        steps_buf: String::new(),
        exts_buf: String::new(),
        dirty: false,
        quit_armed: false,
        show_all: cfg.show_unmatched,
        help: false,
        msg: String::new(),
        quit: false,
    };
    app.rescan();
    app.rule_state.select((!app.rules.is_empty()).then_some(0));

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                app.on_key(key);
            }
        }
    }
    Ok(())
}

impl App {
    fn rescan(&mut self) {
        self.files = plan::collect(&[self.dir.clone()], false, &[]);
        self.recompute();
    }

    fn recompute(&mut self) {
        self.compiled = self.rules.iter().map(Compiled::compile).collect();
        self.entries = plan::plan(&self.files, &self.compiled);
        let visible = self.visible().len();
        let sel = self.file_state.selected().unwrap_or(0).min(visible.saturating_sub(1));
        self.file_state.select((visible > 0).then_some(sel));
    }

    fn visible(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| self.show_all || e.status != Status::NoMatch)
            .collect()
    }

    fn is_selected(&self, e: &Entry) -> bool {
        e.status == Status::Ready && !self.excluded.contains(&e.path)
    }

    fn selected_count(&self) -> usize {
        self.entries.iter().filter(|e| self.is_selected(e)).count()
    }

    fn toggle_current(&mut self) {
        let Some(i) = self.file_state.selected() else { return };
        let Some(e) = self.visible().get(i).map(|e| (*e).clone()) else { return };
        if e.status != Status::Ready {
            self.msg = "only files with a pending rename can be selected".into();
        } else if !self.excluded.remove(&e.path) {
            self.excluded.insert(e.path);
        }
    }

    fn rule(&mut self) -> Option<&mut Rule> {
        let i = self.rule_state.selected()?;
        self.rules.get_mut(i)
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        match self.mode {
            Mode::Normal => {
                self.msg.clear();
                self.normal_key(key.code);
            }
            Mode::Rules => {
                self.msg.clear();
                self.rules_key(key.code);
            }
            Mode::Edit => self.edit_key(key.code),
            Mode::Confirm => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.mode = Mode::Normal;
                        self.apply();
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.mode = Mode::Normal,
                    _ => {}
                }
            }
        }
    }

    fn normal_key(&mut self, code: KeyCode) {
        if code != KeyCode::Char('q') {
            self.quit_armed = false;
        }
        let nfiles = self.visible().len();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.dirty && !self.quit_armed {
                    self.quit_armed = true;
                    self.msg = "unsaved rule changes: press s to save, or q again to discard".into();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => step(&mut self.file_state, nfiles, 1),
            KeyCode::Up | KeyCode::Char('k') => step(&mut self.file_state, nfiles, -1),
            KeyCode::PageDown => page(&mut self.file_state, nfiles, PAGE),
            KeyCode::PageUp => page(&mut self.file_state, nfiles, -PAGE),
            KeyCode::Home => self.file_state.select((nfiles > 0).then_some(0)),
            KeyCode::End => self.file_state.select(nfiles.checked_sub(1)),
            KeyCode::Char(' ') => self.toggle_current(),
            KeyCode::Char('A') => self.excluded.clear(),
            KeyCode::Char('N') => {
                self.excluded = self.entries.iter().filter(|e| e.status == Status::Ready).map(|e| e.path.clone()).collect();
            }
            KeyCode::Char('r') => self.mode = Mode::Rules,
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Char('s') => self.save(),
            KeyCode::Char('t') => {
                self.show_all = !self.show_all;
                self.recompute();
                self.persist_show_all();
            }
            KeyCode::F(5) => {
                self.rescan();
                self.msg = "rescanned".into();
            }
            KeyCode::Char('a') => {
                let n = self.selected_count();
                if n == 0 {
                    self.msg = "nothing selected (space to select)".into();
                } else {
                    self.mode = Mode::Confirm;
                }
            }
            KeyCode::Char('u') => {
                self.msg = match history::undo_last() {
                    Ok(None) => "nothing to undo".into(),
                    Ok(Some((n, skipped))) => {
                        let extra = if skipped.is_empty() { String::new() } else { format!(", skipped {}: {}", skipped.len(), skipped[0]) };
                        format!("restored {n} file(s){extra}")
                    }
                    Err(e) => format!("undo failed: {e}"),
                };
                self.rescan();
            }
            _ => {}
        }
    }

    fn rules_key(&mut self, code: KeyCode) {
        let nrules = self.rules.len();
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Down | KeyCode::Char('j') => step(&mut self.rule_state, nrules, 1),
            KeyCode::Up | KeyCode::Char('k') => step(&mut self.rule_state, nrules, -1),
            KeyCode::Char('J') => self.move_rule(1),
            KeyCode::Char('K') => self.move_rule(-1),
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Char('s') => self.save(),
            KeyCode::Char('e') | KeyCode::Enter if self.rule_state.selected().is_some() => {
                self.field = 1;
                self.start_edit();
            }
            KeyCode::Char('n') => {
                self.rules.push(Rule {
                    name: "new rule".into(),
                    pattern: "(?P<title>.+)".into(),
                    template: "{title}".into(),
                    steps: vec![],
                    extensions: vec!["mkv".into(), "mp4".into(), "avi".into()],
                });
                self.rule_state.select(Some(self.rules.len() - 1));
                self.dirty = true;
                self.field = 0;
                self.start_edit();
            }
            KeyCode::Char('d') => {
                if let Some(i) = self.rule_state.selected() {
                    self.rules.remove(i);
                    self.rule_state.select(i.checked_sub(1).or((!self.rules.is_empty()).then_some(0)));
                    self.dirty = true;
                    self.recompute();
                }
            }
            _ => {}
        }
    }

    /// Writes just this setting, leaving any unsaved rule edits out of the file.
    fn persist_show_all(&mut self) {
        let result = config::load(&self.config_path).and_then(|mut on_disk| {
            on_disk.show_unmatched = self.show_all;
            config::save(&self.config_path, &on_disk)
        });
        if let Err(e) = result {
            self.msg = format!("couldn't save setting: {e}");
        }
    }

    fn save(&mut self) {
        let cfg = Config { show_unmatched: self.show_all, rules: self.rules.clone() };
        self.msg = match config::save(&self.config_path, &cfg) {
            Ok(()) => {
                self.dirty = false;
                format!("saved {}", self.config_path.display())
            }
            Err(e) => format!("save failed: {e}"),
        };
    }

    fn start_edit(&mut self) {
        let (steps, exts) = self.rule().map(|r| (r.steps.join(", "), r.extensions.join(", "))).unwrap_or_default();
        self.steps_buf = steps;
        self.exts_buf = exts;
        self.mode = Mode::Edit;
    }

    fn move_rule(&mut self, delta: isize) {
        let Some(i) = self.rule_state.selected() else { return };
        let j = i as isize + delta;
        if j >= 0 && (j as usize) < self.rules.len() {
            self.rules.swap(i, j as usize);
            self.rule_state.select(Some(j as usize));
            self.dirty = true;
            self.recompute();
        }
    }

    fn edit_key(&mut self, code: KeyCode) {
        let field = self.field;
        match code {
            KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Rules,
            KeyCode::F(1) => self.help = true,
            KeyCode::Tab | KeyCode::Down => self.field = (field + 1) % FIELDS.len(),
            KeyCode::BackTab | KeyCode::Up => self.field = (field + FIELDS.len() - 1) % FIELDS.len(),
            KeyCode::Char(c) => self.edit_text(|s| s.push(c)),
            KeyCode::Backspace => self.edit_text(|s| {
                s.pop();
            }),
            _ => {}
        }
    }

    fn edit_text(&mut self, f: impl FnOnce(&mut String)) {
        let field = self.field;
        let mut steps = std::mem::take(&mut self.steps_buf);
        let mut exts = std::mem::take(&mut self.exts_buf);
        if let Some(rule) = self.rule() {
            match field {
                0 => f(&mut rule.name),
                1 => f(&mut rule.pattern),
                2 => f(&mut rule.template),
                3 => {
                    f(&mut steps);
                    rule.steps = split_list(&steps);
                }
                _ => {
                    f(&mut exts);
                    rule.extensions = split_list(&exts);
                }
            }
        }
        self.steps_buf = steps;
        self.exts_buf = exts;
        self.dirty = true;
        self.recompute();
    }

    fn apply(&mut self) {
        let chosen: Vec<Entry> = self.entries.iter().filter(|e| self.is_selected(e)).cloned().collect();
        let (done, errors) = plan::apply(&chosen);
        let n = done.len();
        let recorded = history::record(done);
        self.msg = match (errors.first(), recorded) {
            (Some(e), _) => format!("renamed {n}, {} error(s): {e}", errors.len()),
            (None, Err(e)) => format!("renamed {n}, but history not saved: {e}"),
            (None, Ok(())) => format!("renamed {n} file(s), press u to undo"),
        };
        self.rescan();
    }
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn page(state: &mut ListState, len: usize, delta: isize) {
    if len > 0 {
        let cur = state.selected().unwrap_or(0) as isize;
        state.select(Some((cur + delta).clamp(0, len as isize - 1) as usize));
    }
}

fn step(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0) as isize;
    state.select(Some((cur + delta).rem_euclid(len as isize) as usize));
}

fn file_name(p: &Path) -> String {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string()
}

fn entry_line(app: &App, e: &Entry, checkbox: bool) -> Line<'static> {
    let old = file_name(&e.path);
    let (arrow, new, color) = match (&e.status, &e.target) {
        (Status::Ready, Some(t)) => (" → ", file_name(t), Color::Green),
        (Status::Conflict(why), Some(t)) => (" ✗ ", format!("{} ({why})", file_name(t)), Color::Yellow),
        (Status::Unchanged, _) => ("", "(unchanged)".into(), Color::DarkGray),
        _ => ("", String::new(), Color::DarkGray),
    };
    let rule = e.rule.and_then(|i| app.rules.get(i)).map(|r| format!("  [{}]", r.name)).unwrap_or_default();
    let old_style = if e.status == Status::NoMatch { Style::new().fg(Color::DarkGray) } else { Style::new() };
    let boxed = match (checkbox, e.status == Status::Ready) {
        (true, true) => if app.excluded.contains(&e.path) { "[ ] " } else { "[x] " },
        (true, false) => "    ",
        _ => "",
    };
    Line::from(vec![
        Span::styled(boxed, Style::new().fg(Color::Green)),
        Span::styled(old, old_style),
        Span::raw(arrow),
        Span::styled(new, Style::new().fg(color)),
        Span::styled(rule, Style::new().fg(Color::DarkGray)),
    ])
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Percentage(w)]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([Constraint::Percentage(h)]).flex(Flex::Center).areas(area);
    area
}

fn draw(f: &mut Frame, app: &mut App) {
    let [main, status] = Layout::vertical([Constraint::Min(6), Constraint::Length(1)]).areas(f.area());

    let items: Vec<ListItem> = app.visible().iter().map(|e| ListItem::new(entry_line(app, e, true))).collect();
    let ready = app.entries.iter().filter(|e| e.status == Status::Ready).count();
    let title = format!(" {} — {} of {ready} selected ", app.dir.display(), app.selected_count());
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, main, &mut app.file_state);

    if matches!(app.mode, Mode::Rules | Mode::Edit) {
        draw_rules_modal(f, app, main);
    }
    if app.mode == Mode::Confirm {
        draw_confirm(f, app, main);
    }
    if app.help {
        draw_help(f, app, centered(main, 80, 90));
    }

    let hints: &[(&str, &str)] = match app.mode {
        Mode::Normal => &[
            ("j/k", "move"),
            ("space", "select"),
            ("A", "all"),
            ("N", "none"),
            ("a", "apply"),
            ("u", "undo"),
            ("r", "rules"),
            ("t", if app.show_all { "hide unmatched" } else { "show unmatched" }),
            ("?", "help"),
            ("q", "quit"),
        ],
        Mode::Rules => &[
            ("j/k", "select"),
            ("e", "edit"),
            ("n", "new"),
            ("d", "delete"),
            ("J/K", "reorder"),
            ("s", "save"),
            ("?", "placeholders"),
            ("Esc", "close"),
        ],
        Mode::Edit => &[("Tab/Shift-Tab", "field"), ("F1", "placeholders"), ("Esc/Enter", "done")],
        Mode::Confirm => &[("y", "yes"), ("n/Esc", "cancel")],
    };
    let line = if app.msg.is_empty() {
        let key_style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let spans: Vec<Span> = hints
            .iter()
            .flat_map(|(k, d)| {
                [
                    Span::styled(k.to_string(), key_style),
                    Span::styled(format!(" {d}  "), Style::new().fg(Color::Gray)),
                ]
            })
            .collect();
        Line::from(spans)
    } else {
        Line::styled(app.msg.clone(), Style::new().fg(Color::Yellow))
    };
    f.render_widget(Paragraph::new(line), status);
}

fn draw_confirm(f: &mut Frame, app: &App, area: Rect) {
    let n = app.selected_count();
    let skipped = app.entries.iter().filter(|e| e.status == Status::Ready).count() - n;
    let key = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(format!("Rename {n} file(s)?")).centered(),
        Line::styled(
            if skipped > 0 { format!("{skipped} deselected file(s) will be left alone") } else { String::new() },
            Style::new().fg(Color::DarkGray),
        )
        .centered(),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y", key),
            Span::raw(" Yes     "),
            Span::styled("n", key),
            Span::raw(" No"),
        ])
        .centered(),
    ];
    let [area] = Layout::horizontal([Constraint::Length(44)]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([Constraint::Length(8)]).flex(Flex::Center).areas(area);
    f.render_widget(Clear, area);
    let block = Block::bordered().title(" Apply ").border_style(Style::new().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [_, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    f.render_widget(Paragraph::new(lines), body);
}

fn draw_rules_modal(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_widget(Clear, area);
    let title = if app.dirty { " Rules (modified, s to save) " } else { " Rules " };
    let outer = Block::bordered().title(title).border_style(Style::new().fg(Color::Cyan));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let [top, preview_area] = Layout::vertical([Constraint::Length(9), Constraint::Min(3)]).areas(inner);
    let [list_area, editor_area] =
        Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(72)]).areas(top);

    let rule_items: Vec<ListItem> = app
        .rules
        .iter()
        .zip(&app.compiled)
        .enumerate()
        .map(|(i, (r, c))| {
            let style = if c.is_err() { Style::new().fg(Color::Red) } else { Style::new() };
            ListItem::new(format!("{}. {}", i + 1, r.name)).style(style)
        })
        .collect();
    let rules = List::new(rule_items)
        .block(Block::bordered().title(" Order (first match wins) "))
        .highlight_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    f.render_stateful_widget(rules, list_area, &mut app.rule_state);

    let editing = app.mode == Mode::Edit;
    let sel = app.rule_state.selected();
    let mut lines = Vec::new();
    if let Some(i) = sel {
        let rule = &app.rules[i];
        let (steps, exts) = if editing {
            (app.steps_buf.clone(), app.exts_buf.clone())
        } else {
            (rule.steps.join(", "), rule.extensions.join(", "))
        };
        let values = [rule.name.clone(), rule.pattern.clone(), rule.template.clone(), steps, exts];
        for (n, label) in FIELDS.iter().enumerate() {
            let active = editing && app.field == n;
            let style = if active { Style::new().fg(Color::Cyan) } else { Style::new() };
            let cursor = if active { "▏" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<9}"), Style::new().fg(Color::DarkGray)),
                Span::styled(format!("{}{cursor}", values[n]), style),
            ]));
        }
        if let Some(Err(e)) = app.compiled.get(i) {
            lines.push(Line::styled(e.clone(), Style::new().fg(Color::Red)));
        }
    } else {
        lines.push(Line::raw("No rules. Press n to create one."));
    }
    let etitle = if editing { " Edit " } else { " Rule " };
    f.render_widget(Paragraph::new(lines).block(Block::bordered().title(etitle)), editor_area);

    let matched: Vec<ListItem> = app
        .entries
        .iter()
        .filter(|e| sel.is_some() && e.rule == sel)
        .map(|e| ListItem::new(entry_line(app, e, false)))
        .collect();
    let ptitle = format!(" Preview: {} file(s) matched by this rule ", matched.len());
    f.render_widget(List::new(matched).block(Block::bordered().title(ptitle)), preview_area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Clear, area);
    let head = |t: &str| Line::styled(t.to_string(), Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let code = |t: &str| Span::styled(t.to_string(), Style::new().fg(Color::Green));

    let groups: Vec<String> = app
        .rule_state
        .selected()
        .and_then(|i| app.rules.get(i))
        .and_then(|r| Regex::new(&r.pattern).ok())
        .map(|re| re.capture_names().flatten().map(|g| format!("{{{g}}}")).collect())
        .unwrap_or_default();
    let groups_line = if groups.is_empty() {
        "none yet: add named groups like (?P<title>.+) to the pattern".to_string()
    } else {
        groups.join("  ")
    };

    let lines = vec![
        head("Placeholders (Template field)"),
        Line::from(vec![code("  {name}"), Span::raw("          value captured by the group (?P<name>…) in the Pattern")]),
        Line::from(vec![code("  {name|filter}"), Span::raw("   transform the value; chain with more |")]),
        Line::from(vec![Span::raw("  Groups in the selected rule: "), Span::styled(groups_line, Style::new().fg(Color::Yellow))]),
        Line::raw(""),
        head("Filters"),
        Line::from(vec![code("  upper  lower  title  trim"), Span::raw("   change case / strip spaces")]),
        Line::from(vec![code("  pad2"), Span::raw("                       left-pad with zeros to 2 digits (2 → 02)")]),
        Line::raw(""),
        head("Steps (cleanup before the pattern runs, in order)"),
        Line::from(vec![code("  strip_brackets"), Span::raw("         remove [anything in square brackets]")]),
        Line::from(vec![code("  dots_to_spaces"), Span::raw("         Example.Show → Example Show")]),
        Line::from(vec![code("  underscores_to_spaces")]),
        Line::from(vec![code("  collapse_spaces"), Span::raw("        squeeze repeated whitespace")]),
        Line::raw(""),
        head("Formats"),
        Line::raw("  Comma separated extensions the rule applies to (mkv, mp4, avi). Empty = any."),
        Line::raw(""),
        head("Example"),
        Line::from(vec![Span::raw("  Pattern   "), code("^(?P<title>.+?) S(?P<season>\\d+)E(?P<episode>\\d+)")]),
        Line::from(vec![Span::raw("  Template  "), code("{title} - S{season|pad2}E{episode|pad2}")]),
        Line::raw("  Example Show S2E6  →  Example Show - S02E06"),
        Line::raw(""),
        Line::styled("press any key to close", Style::new().fg(Color::DarkGray)),
    ];
    let block = Block::bordered().title(" Help ").border_style(Style::new().fg(Color::Yellow));
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}
