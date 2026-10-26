//! The window's event loop: turns `Action`s from `App::handle_key` into I/O and
//! reports the outcome back to the app.
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::app::{Action, App};
use crate::config::{self, Config};
use crate::{history, plan, theme, ui};

struct Ctx {
    dir: PathBuf,
    config_path: PathBuf,
    journal: PathBuf,
}

pub fn run(cfg: Config, config_path: PathBuf, dir: PathBuf, journal: PathBuf) -> Result<()> {
    // An absolute path reads better in the header and keeps undo history unambiguous.
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let themes = theme::load_themes(&config::themes_dir(&config_path), &cfg.themes)?;
    let mut app = App::new(cfg, dir.clone(), themes)
        .map_err(|e| anyhow!("{}: {e}", config_path.display()))?;
    let ctx = Ctx {
        dir,
        config_path,
        journal,
    };
    rescan(&mut app, &ctx);

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, &ctx);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App, ctx: &Ctx) -> Result<()> {
    while !app.quit {
        app.tick(Instant::now());
        terminal.draw(|f| ui::draw(f, app))?;
        // Wake up regularly so status messages can expire.
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(action) = app.handle_key(key)
        {
            perform(terminal, app, ctx, action);
        }
    }
    Ok(())
}

/// Themes from the files beside the config (plus its `themes:` map), read fresh each time.
fn load_themes(ctx: &Ctx) -> Result<Vec<theme::Theme>> {
    let cfg = config::load(&ctx.config_path)?;
    theme::load_themes(&config::themes_dir(&ctx.config_path), &cfg.themes)
}

fn rescan(app: &mut App, ctx: &Ctx) {
    let files = plan::collect(std::slice::from_ref(&ctx.dir), false, &[]);
    let existing = plan::existing(&files);
    app.set_files(files, existing);
}

fn perform(terminal: &mut DefaultTerminal, app: &mut App, ctx: &Ctx, action: Action) {
    match action {
        Action::Rescan => {
            rescan(app, ctx);
            app.ok("rescanned");
        }
        Action::Apply(entries) => {
            let (done, errors) = plan::apply(&entries);
            let n = done.len();
            let recorded = history::record(&ctx.journal, done);
            match (errors.first(), recorded) {
                (Some(e), _) => app.fail(format!("renamed {n}, {} error(s): {e}", errors.len())),
                (None, Err(e)) => app.fail(format!("renamed {n}, but history not saved: {e}")),
                (None, Ok(())) => app.ok(format!("renamed {n} file(s), press u to undo")),
            }
            rescan(app, ctx);
        }
        Action::Undo => {
            match history::undo_last(&ctx.journal) {
                Ok(None) => app.fail("nothing to undo"),
                Ok(Some((n, skipped))) if skipped.is_empty() => {
                    app.ok(format!("restored {n} file(s)"))
                }
                Ok(Some((n, skipped))) => {
                    app.fail(format!(
                        "restored {n}, skipped {}: {}",
                        skipped.len(),
                        skipped[0]
                    ));
                }
                Err(e) => app.fail(format!("undo failed: {e}")),
            }
            rescan(app, ctx);
        }
        Action::SaveRules(rules) => match config::set_rules(&ctx.config_path, &rules) {
            Ok(()) => {
                app.rules_saved();
                app.ok(format!("saved {}", ctx.config_path.display()));
            }
            Err(e) => app.fail(format!("save failed: {e:#}")),
        },
        Action::SaveShowUnmatched(on) => {
            if let Err(e) = config::set_value(&ctx.config_path, "show_unmatched", &on) {
                app.fail(format!("couldn't save setting: {e:#}"));
            }
        }
        Action::SaveTheme { name, transparent } => {
            let saved =
                config::set_value(&ctx.config_path, "theme", &name).and_then(
                    |()| match transparent {
                        Some(on) => {
                            config::set_value(&ctx.config_path, "transparent_background", &on)
                        }
                        None => Ok(()),
                    },
                );
            match saved {
                Ok(()) => app.ok(format!("theme: {name}")),
                Err(e) => app.fail(format!("couldn't save theme: {e:#}")),
            }
        }
        Action::OpenThemes => match load_themes(ctx) {
            Ok(themes) => app.open_themes(themes),
            Err(e) => app.fail(format!("{e:#}")),
        },
        Action::EditConfig => open_in_editor(terminal, app, ctx),
    }
}

/// Hands the terminal to `$VISUAL`/`$EDITOR` (default `vi`), then reloads the config.
fn open_in_editor(terminal: &mut DefaultTerminal, app: &mut App, ctx: &Ctx) {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|e| !e.trim().is_empty()))
        .unwrap_or_else(|| "vi".into());
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");

    ratatui::restore();
    let status = Command::new(program)
        .args(parts)
        .arg(&ctx.config_path)
        .status();
    *terminal = ratatui::init();

    match status {
        Ok(s) if s.success() => {
            let reloaded = config::load(&ctx.config_path).and_then(|cfg| {
                let themes =
                    theme::load_themes(&config::themes_dir(&ctx.config_path), &cfg.themes)?;
                app.apply_config(cfg, themes).map_err(|e| anyhow!(e))
            });
            match reloaded {
                Ok(()) => app.ok("config reloaded"),
                Err(e) => app.fail(format!("config not reloaded: {e:#}")),
            }
        }
        Ok(s) => app.fail(format!("editor exited with {s}; config not reloaded")),
        Err(e) => app.fail(format!("couldn't run '{editor}': {e}")),
    }
}
