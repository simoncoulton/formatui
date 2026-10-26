//! UI state and all key handling. Pure: no I/O and no rendering. Keys return an
//! `Action`; `tui.rs` performs it and reports back through `ok`, `fail` and friends.
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{Config, Rule};
use crate::engine::Compiled;
use crate::plan::{self, Entry, Status};
use crate::theme::{self, Theme};

pub const FIELDS: [&str; 5] = ["Name", "Pattern", "Template", "Steps", "Formats"];
const PAGE: isize = 10;
const STATUS_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Confirm {
    Apply,
    DeleteRule,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Normal,
    /// Rules modal, browsing the rule list.
    Rules,
    /// Rules modal, typing into a field. Plain characters are text here.
    Edit,
    Confirm(Confirm),
    Themes,
}

/// Side effects a key asks for; `tui.rs` performs them.
#[derive(Debug, PartialEq)]
pub enum Action {
    Rescan,
    Apply(Vec<Entry>),
    Undo,
    SaveRules(Vec<Rule>),
    SaveShowUnmatched(bool),
    /// `transparent` is only set when the picker changed it.
    SaveTheme {
        name: String,
        transparent: Option<bool>,
    },
    /// Re-read the theme files, then call `App::open_themes`.
    OpenThemes,
    EditConfig,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
    at: Instant,
}

/// What to restore if the theme picker is cancelled.
struct ThemeBackup {
    name: String,
    transparent: bool,
}

pub struct App {
    pub dir: PathBuf,
    pub rules: Vec<Rule>,
    pub compiled: Vec<Result<Compiled, String>>,
    files: Vec<PathBuf>,
    existing: HashSet<String>,
    pub entries: Vec<Entry>,
    /// Ready files the user unticked; everything else that is ready gets renamed.
    pub excluded: HashSet<PathBuf>,
    pub file_sel: Option<usize>,
    pub rule_sel: Option<usize>,
    pub mode: Mode,
    pub field: usize,
    pub steps_buf: String,
    pub exts_buf: String,
    /// Rules changed in the UI and not yet saved.
    pub dirty: bool,
    quit_armed: bool,
    pub show_all: bool,
    pub help: bool,
    pub help_scroll: u16,
    pub status: Option<Notice>,
    pub quit: bool,
    pub themes: Vec<Theme>,
    pub theme_idx: usize,
    pub transparent: bool,
    /// The palette in use, already adjusted for transparency.
    pub theme: Theme,
    theme_backup: Option<ThemeBackup>,
}

impl App {
    pub fn new(cfg: Config, dir: PathBuf, themes: Vec<Theme>) -> Result<Self, String> {
        let theme_idx = find_theme(&themes, &cfg.theme)?;
        let mut app = Self {
            dir,
            rules: cfg.rules,
            compiled: vec![],
            files: vec![],
            existing: HashSet::new(),
            entries: vec![],
            excluded: HashSet::new(),
            file_sel: None,
            rule_sel: None,
            mode: Mode::Normal,
            field: 0,
            steps_buf: String::new(),
            exts_buf: String::new(),
            dirty: false,
            quit_armed: false,
            show_all: cfg.show_unmatched,
            help: false,
            help_scroll: 0,
            status: None,
            quit: false,
            theme: themes[theme_idx].clone(),
            themes,
            theme_idx,
            transparent: cfg.transparent_background,
            theme_backup: None,
        };
        app.refresh_theme();
        app.rule_sel = (!app.rules.is_empty()).then_some(0);
        app.recompute();
        Ok(app)
    }

    // ---- results reported back by tui.rs -------------------------------------------------

    pub fn ok(&mut self, text: impl Into<String>) {
        self.status = Some(Notice {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    pub fn fail(&mut self, text: impl Into<String>) {
        self.status = Some(Notice {
            text: text.into(),
            error: true,
            at: Instant::now(),
        });
    }

    /// Drops an expired status message.
    pub fn tick(&mut self, now: Instant) {
        if self
            .status
            .as_ref()
            .is_some_and(|s| now.duration_since(s.at) > STATUS_TTL)
        {
            self.status = None;
        }
    }

    pub fn set_files(&mut self, files: Vec<PathBuf>, existing: HashSet<String>) {
        self.files = files;
        self.existing = existing;
        self.recompute();
    }

    pub fn rules_saved(&mut self) {
        self.dirty = false;
    }

    /// Takes over rules and settings from a freshly loaded config.
    pub fn apply_config(&mut self, cfg: Config, themes: Vec<Theme>) -> Result<(), String> {
        let idx = find_theme(&themes, &cfg.theme)?;
        self.themes = themes;
        self.theme_idx = idx;
        self.transparent = cfg.transparent_background;
        self.show_all = cfg.show_unmatched;
        self.rules = cfg.rules;
        self.dirty = false;
        let last = self.rules.len().checked_sub(1);
        self.rule_sel = last.map(|l| self.rule_sel.unwrap_or(0).min(l));
        self.refresh_theme();
        self.recompute();
        Ok(())
    }

    // ---- derived state -------------------------------------------------------------------

    fn refresh_theme(&mut self) {
        self.theme = self.themes[self.theme_idx]
            .clone()
            .transparent(self.transparent);
    }

    fn recompute(&mut self) {
        self.compiled = self.rules.iter().map(Compiled::compile).collect();
        self.entries = plan::plan(&self.files, &self.compiled, &self.existing);
        let visible = self.visible().len();
        self.file_sel = (visible > 0).then(|| self.file_sel.unwrap_or(0).min(visible - 1));
    }

    pub fn visible(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| self.show_all || e.status != Status::NoMatch)
            .collect()
    }

    pub fn is_selected(&self, e: &Entry) -> bool {
        e.status == Status::Ready && !self.excluded.contains(&e.path)
    }

    pub fn selected_count(&self) -> usize {
        self.entries.iter().filter(|e| self.is_selected(e)).count()
    }

    pub fn ready_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == Status::Ready)
            .count()
    }

    fn rule(&mut self) -> Option<&mut Rule> {
        let i = self.rule_sel?;
        self.rules.get_mut(i)
    }

    // ---- keys ----------------------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        if self.help {
            self.key_help(key.code);
            return None;
        }
        match self.mode {
            Mode::Normal => self.key_normal(key.code),
            Mode::Rules => self.key_rules(key.code),
            Mode::Edit => {
                self.key_edit(key.code);
                None
            }
            Mode::Confirm(what) => self.key_confirm(what, key.code),
            Mode::Themes => self.key_themes(key.code),
        }
    }

    fn key_help(&mut self, code: KeyCode) {
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.help_scroll = self.help_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
            KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
            KeyCode::Home | KeyCode::Char('g') => self.help_scroll = 0,
            KeyCode::Esc | KeyCode::Char('q' | '?') | KeyCode::F(1) | KeyCode::Enter => {
                self.help = false;
                self.help_scroll = 0;
            }
            _ => {}
        }
    }

    fn key_normal(&mut self, code: KeyCode) -> Option<Action> {
        if code != KeyCode::Char('q') {
            self.quit_armed = false;
        }
        let n = self.visible().len();
        match code {
            KeyCode::Char('q') => {
                if self.dirty && !self.quit_armed {
                    self.quit_armed = true;
                    self.fail("unsaved rule changes: press s to save, or q again to discard");
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Esc => self.status = None,
            KeyCode::Down | KeyCode::Char('j') => step(&mut self.file_sel, n, 1),
            KeyCode::Up | KeyCode::Char('k') => step(&mut self.file_sel, n, -1),
            KeyCode::PageDown => page(&mut self.file_sel, n, PAGE),
            KeyCode::PageUp => page(&mut self.file_sel, n, -PAGE),
            KeyCode::Home | KeyCode::Char('g') => self.file_sel = (n > 0).then_some(0),
            KeyCode::End | KeyCode::Char('G') => self.file_sel = n.checked_sub(1),
            KeyCode::Char(' ') => self.toggle_current(),
            KeyCode::Char('A') => self.excluded.clear(),
            KeyCode::Char('N') => {
                self.excluded = self
                    .entries
                    .iter()
                    .filter(|e| e.status == Status::Ready)
                    .map(|e| e.path.clone())
                    .collect();
            }
            KeyCode::Char('r') => self.mode = Mode::Rules,
            KeyCode::Char('T') => return Some(Action::OpenThemes),
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Char('s') if self.dirty => return Some(Action::SaveRules(self.rules.clone())),
            KeyCode::Char('t') => {
                self.show_all = !self.show_all;
                self.recompute();
                return Some(Action::SaveShowUnmatched(self.show_all));
            }
            KeyCode::Char('E') => return self.request_config_edit(),
            KeyCode::F(5) => return Some(Action::Rescan),
            KeyCode::Char('a') => {
                if self.selected_count() == 0 {
                    self.fail("nothing selected (space to select)");
                } else {
                    self.mode = Mode::Confirm(Confirm::Apply);
                }
            }
            KeyCode::Char('u') => return Some(Action::Undo),
            _ => {}
        }
        None
    }

    fn key_rules(&mut self, code: KeyCode) -> Option<Action> {
        let n = self.rules.len();
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Down | KeyCode::Char('j') => step(&mut self.rule_sel, n, 1),
            KeyCode::Up | KeyCode::Char('k') => step(&mut self.rule_sel, n, -1),
            KeyCode::Home | KeyCode::Char('g') => self.rule_sel = (n > 0).then_some(0),
            KeyCode::End | KeyCode::Char('G') => self.rule_sel = n.checked_sub(1),
            KeyCode::Char('J') => self.move_rule(1),
            KeyCode::Char('K') => self.move_rule(-1),
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Char('s') => return Some(Action::SaveRules(self.rules.clone())),
            KeyCode::Char('E') => return self.request_config_edit(),
            KeyCode::Char('e') | KeyCode::Enter if self.rule_sel.is_some() => {
                self.field = 1;
                self.start_edit();
            }
            KeyCode::Char('n') => {
                self.rules.push(Rule {
                    name: "new rule".into(),
                    pattern: "(?P<title>.+)".into(),
                    template: "{title}".into(),
                    steps: vec![],
                    extensions: ["mkv", "mp4", "avi"].map(String::from).to_vec(),
                });
                self.rule_sel = Some(self.rules.len() - 1);
                self.dirty = true;
                self.field = 0;
                self.start_edit();
            }
            KeyCode::Char('d') if self.rule_sel.is_some() => {
                self.mode = Mode::Confirm(Confirm::DeleteRule);
            }
            _ => {}
        }
        None
    }

    fn key_edit(&mut self, code: KeyCode) {
        let field = self.field;
        match code {
            KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Rules,
            KeyCode::F(1) => self.help = true,
            KeyCode::Tab | KeyCode::Down => self.field = (field + 1) % FIELDS.len(),
            KeyCode::BackTab | KeyCode::Up => {
                self.field = (field + FIELDS.len() - 1) % FIELDS.len();
            }
            KeyCode::Char(c) => self.edit_text(|s| s.push(c)),
            KeyCode::Backspace => self.edit_text(|s| {
                s.pop();
            }),
            _ => {}
        }
    }

    /// `y` confirms; anything else cancels.
    fn key_confirm(&mut self, what: Confirm, code: KeyCode) -> Option<Action> {
        self.mode = if what == Confirm::DeleteRule {
            Mode::Rules
        } else {
            Mode::Normal
        };
        if code != KeyCode::Char('y') {
            return None;
        }
        match what {
            Confirm::Apply => {
                let chosen = self
                    .entries
                    .iter()
                    .filter(|e| self.is_selected(e))
                    .cloned()
                    .collect();
                Some(Action::Apply(chosen))
            }
            Confirm::DeleteRule => {
                if let Some(i) = self.rule_sel {
                    self.rules.remove(i);
                    self.rule_sel = i.checked_sub(1).or((!self.rules.is_empty()).then_some(0));
                    self.dirty = true;
                    self.recompute();
                }
                None
            }
        }
    }

    fn key_themes(&mut self, code: KeyCode) -> Option<Action> {
        let last = self.themes.len().saturating_sub(1);
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_theme(self.theme_idx.saturating_add(1).min(last));
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_theme(self.theme_idx.saturating_sub(1)),
            KeyCode::Home | KeyCode::Char('g') => self.move_theme(0),
            KeyCode::End | KeyCode::Char('G') => self.move_theme(last),
            KeyCode::Char('b') => {
                self.transparent = !self.transparent;
                self.refresh_theme();
            }
            KeyCode::Enter => {
                let was = self
                    .theme_backup
                    .take()
                    .map_or(self.transparent, |b| b.transparent);
                self.mode = Mode::Normal;
                return Some(Action::SaveTheme {
                    name: self.themes[self.theme_idx].name.clone(),
                    transparent: (self.transparent != was).then_some(self.transparent),
                });
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                if let Some(b) = self.theme_backup.take() {
                    self.theme_idx = find_theme(&self.themes, &b.name).unwrap_or(0);
                    self.transparent = b.transparent;
                    self.refresh_theme();
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        None
    }

    // ---- helpers -------------------------------------------------------------------------

    /// Opens the picker over a freshly loaded theme list, keeping the current theme selected.
    pub fn open_themes(&mut self, themes: Vec<Theme>) {
        let current = self.themes[self.theme_idx].name.clone();
        self.themes = themes;
        self.theme_idx = find_theme(&self.themes, &current).unwrap_or(0);
        self.theme_backup = Some(ThemeBackup {
            name: current,
            transparent: self.transparent,
        });
        self.refresh_theme();
        self.mode = Mode::Themes;
    }

    fn move_theme(&mut self, to: usize) {
        self.theme_idx = to;
        self.refresh_theme();
    }

    fn request_config_edit(&mut self) -> Option<Action> {
        if self.dirty {
            self.fail("unsaved rule changes: save (s) before editing the config file");
            None
        } else {
            Some(Action::EditConfig)
        }
    }

    fn toggle_current(&mut self) {
        let Some(i) = self.file_sel else { return };
        let Some(e) = self.visible().get(i).map(|e| (*e).clone()) else {
            return;
        };
        if e.status != Status::Ready {
            self.fail("only files with a pending rename can be selected");
        } else if !self.excluded.remove(&e.path) {
            self.excluded.insert(e.path);
        }
    }

    fn start_edit(&mut self) {
        let (steps, exts) = self
            .rule()
            .map(|r| (r.steps.join(", "), r.extensions.join(", ")))
            .unwrap_or_default();
        self.steps_buf = steps;
        self.exts_buf = exts;
        self.mode = Mode::Edit;
    }

    fn move_rule(&mut self, delta: isize) {
        let Some(i) = self.rule_sel else { return };
        let j = i as isize + delta;
        if j >= 0 && (j as usize) < self.rules.len() {
            self.rules.swap(i, j as usize);
            self.rule_sel = Some(j as usize);
            self.dirty = true;
            self.recompute();
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
}

fn find_theme(themes: &[Theme], name: &str) -> Result<usize, String> {
    let want = theme::normalize(name);
    themes
        .iter()
        .position(|t| theme::normalize(&t.name) == want)
        .ok_or_else(|| {
            let names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
            format!("unknown theme '{name}' (available: {})", names.join(", "))
        })
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn step(sel: &mut Option<usize>, len: usize, delta: isize) {
    *sel = (len > 0).then(|| (sel.unwrap_or(0) as isize + delta).rem_euclid(len as isize) as usize);
}

fn page(sel: &mut Option<usize>, len: usize, delta: isize) {
    *sel =
        (len > 0).then(|| (sel.unwrap_or(0) as isize + delta).clamp(0, len as isize - 1) as usize);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn press(app: &mut App, c: KeyCode) -> Option<Action> {
        app.handle_key(KeyEvent::new_with_kind(
            c,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ))
    }

    fn chars(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    /// An app over synthetic paths; nothing here touches the filesystem.
    pub(crate) fn app() -> App {
        let themes = theme::all_themes(&Default::default()).unwrap();
        let mut app = App::new(Config::default(), PathBuf::from("d"), themes).unwrap();
        let files = [
            "d/Show.2024.S01E01.1080p.mkv",
            "d/Show.2024.S01E02.1080p.mkv",
            "d/Some.Film.1999.1080p.mp4",
            "d/notes.txt",
        ];
        app.set_files(files.iter().map(PathBuf::from).collect(), HashSet::new());
        app
    }

    #[test]
    fn space_toggles_and_only_ready_files_count() {
        let mut a = app();
        assert_eq!((a.selected_count(), a.ready_count()), (3, 3));
        press(&mut a, KeyCode::Char(' '));
        assert_eq!(a.selected_count(), 2);
        press(&mut a, KeyCode::Char('N'));
        assert_eq!(a.selected_count(), 0);
        press(&mut a, KeyCode::Char('A'));
        assert_eq!(a.selected_count(), 3);
        press(&mut a, KeyCode::End);
        press(&mut a, KeyCode::Char(' '));
        assert!(
            a.status.as_ref().is_some_and(|s| s.error),
            "unmatched files can't be ticked"
        );
    }

    #[test]
    fn apply_asks_first_and_anything_but_y_cancels() {
        let mut a = app();
        assert_eq!(press(&mut a, KeyCode::Char('a')), None);
        assert_eq!(a.mode, Mode::Confirm(Confirm::Apply));
        assert_eq!(press(&mut a, KeyCode::Char('x')), None);
        assert_eq!(a.mode, Mode::Normal);

        press(&mut a, KeyCode::Char('a'));
        let Some(Action::Apply(chosen)) = press(&mut a, KeyCode::Char('y')) else {
            panic!("y should confirm");
        };
        assert_eq!(chosen.len(), 3);
    }

    #[test]
    fn apply_with_nothing_selected_says_so() {
        let mut a = app();
        press(&mut a, KeyCode::Char('N'));
        press(&mut a, KeyCode::Char('a'));
        assert_eq!(a.mode, Mode::Normal);
        assert!(a.status.as_ref().is_some_and(|s| s.error));
    }

    #[test]
    fn esc_does_not_quit_but_q_does() {
        let mut a = app();
        press(&mut a, KeyCode::Esc);
        assert!(!a.quit);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn quitting_with_unsaved_rules_warns_once() {
        let mut a = app();
        press(&mut a, KeyCode::Char('r'));
        press(&mut a, KeyCode::Char('K'));
        press(&mut a, KeyCode::Char('J'));
        assert!(a.dirty);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('q'));
        assert!(!a.quit);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn text_fields_eat_letters() {
        let mut a = app();
        press(&mut a, KeyCode::Char('r'));
        press(&mut a, KeyCode::Char('n'));
        assert!(a.mode == Mode::Edit);
        a.rules.last_mut().unwrap().name.clear();
        chars(&mut a, "quit? dEsr");
        assert_eq!(a.rules.last().unwrap().name, "quit? dEsr");
        assert!(!a.quit && a.mode == Mode::Edit && !a.help);
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Rules);
    }

    #[test]
    fn deleting_a_rule_asks_first() {
        let mut a = app();
        press(&mut a, KeyCode::Char('r'));
        press(&mut a, KeyCode::Char('d'));
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.rules.len(), a.mode), (2, Mode::Rules));

        press(&mut a, KeyCode::Char('d'));
        press(&mut a, KeyCode::Char('y'));
        assert_eq!(a.rules.len(), 1);
        assert!(a.dirty);
        assert_eq!(a.mode, Mode::Rules);
    }

    #[test]
    fn config_edit_is_refused_while_rules_are_unsaved() {
        let mut a = app();
        assert_eq!(press(&mut a, KeyCode::Char('E')), Some(Action::EditConfig));
        press(&mut a, KeyCode::Char('r'));
        press(&mut a, KeyCode::Char('J'));
        assert_eq!(press(&mut a, KeyCode::Char('E')), None);
        assert!(a.status.as_ref().is_some_and(|s| s.error));
    }

    #[test]
    fn toggling_unmatched_asks_to_persist_the_setting() {
        let mut a = app();
        assert_eq!(
            press(&mut a, KeyCode::Char('t')),
            Some(Action::SaveShowUnmatched(false))
        );
        assert_eq!(a.visible().len(), 3);
    }

    #[test]
    fn theme_picker_previews_restores_and_saves() {
        let mut a = app();
        let original = a.theme.clone();
        let open = |a: &mut App| {
            assert_eq!(press(a, KeyCode::Char('T')), Some(Action::OpenThemes));
            a.open_themes(theme::all_themes(&Default::default()).unwrap());
        };
        open(&mut a);
        press(&mut a, KeyCode::Char('j'));
        assert_ne!(a.theme, original);
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.theme.clone(), a.theme_idx), (original, 0));

        open(&mut a);
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('b'));
        let saved = press(&mut a, KeyCode::Enter);
        assert_eq!(
            saved,
            Some(Action::SaveTheme {
                name: "tokyo-night".into(),
                transparent: Some(false)
            })
        );
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn theme_picker_cursor_is_bounded_and_saves_only_changes() {
        let mut a = app();
        a.open_themes(theme::all_themes(&Default::default()).unwrap());
        press(&mut a, KeyCode::Up);
        assert_eq!(a.theme_idx, 0, "no wrap at the top");
        press(&mut a, KeyCode::Char('G'));
        let last = a.themes.len() - 1;
        press(&mut a, KeyCode::Down);
        assert_eq!(a.theme_idx, last, "no wrap at the bottom");
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.theme_idx, 0);
        let Some(Action::SaveTheme { transparent, .. }) = press(&mut a, KeyCode::Enter) else {
            panic!("enter saves");
        };
        assert_eq!(transparent, None, "background untouched");
    }

    #[test]
    fn unknown_theme_names_list_the_choices() {
        let themes = theme::all_themes(&Default::default()).unwrap();
        let cfg = Config {
            theme: "nope".into(),
            ..Config::default()
        };
        let err = App::new(cfg, PathBuf::from("d"), themes).err().unwrap();
        assert!(
            err.contains("unknown theme 'nope'") && err.contains("tokyo-night"),
            "{err}"
        );
    }

    #[test]
    fn status_messages_expire() {
        let mut a = app();
        a.ok("done");
        a.tick(Instant::now());
        assert!(a.status.is_some());
        a.tick(Instant::now() + Duration::from_secs(30));
        assert!(a.status.is_none());
    }

    #[test]
    fn navigation_wraps_pages_and_jumps() {
        let mut a = app();
        press(&mut a, KeyCode::Up);
        assert_eq!(a.file_sel, Some(3));
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.file_sel, Some(0));
        press(&mut a, KeyCode::PageDown);
        assert_eq!(a.file_sel, Some(3));
        press(&mut a, KeyCode::PageUp);
        assert_eq!(a.file_sel, Some(0));
        press(&mut a, KeyCode::Char('G'));
        assert_eq!(a.file_sel, Some(3));
    }
}
