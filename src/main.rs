mod app;
mod config;
mod engine;
mod history;
mod plan;
mod theme;
mod tui;
mod ui;

use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use engine::Compiled;
use plan::Status;

#[derive(Parser)]
#[command(version, about = "Rename files with ordered regex + template rules")]
struct Cli {
    /// Config file (default: ~/.config/formatui/config.yml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Only use these rules (comma separated names; config order still decides priority)
    #[arg(long, global = true, value_delimiter = ',')]
    rules: Vec<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the interactive UI (default when no command is given)
    Tui {
        /// Directory to load files from
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Rename files without the UI. Dry run unless --apply is passed
    Run {
        /// Files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Descend into subdirectories
        #[arg(short, long)]
        recursive: bool,
        /// Only include these extensions (comma separated)
        #[arg(short, long, value_delimiter = ',')]
        ext: Vec<String>,
        /// Perform the renames
        #[arg(long)]
        apply: bool,
    },
    /// Revert the last applied batch
    Undo,
    /// `formatui <dir>` opens the UI on that directory
    #[command(external_subcommand)]
    Open(Vec<OsString>),
    /// Show what the rules would do to a single file name
    Test { name: String },
    /// List configured rules
    Rules,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(config::default_path);
    let mut cfg = config::load(&config_path)?;
    let command = match cli.command {
        None => Command::Tui { dir: ".".into() },
        Some(Command::Open(args)) => match args.as_slice() {
            [dir] => Command::Tui { dir: dir.into() },
            [_, extra, ..] => bail!("unexpected argument '{}'", extra.to_string_lossy()),
            [] => Command::Tui { dir: ".".into() },
        },
        Some(other) => other,
    };
    if !cli.rules.is_empty() {
        if matches!(command, Command::Tui { .. }) {
            bail!("--rules isn't supported in the TUI (saving would drop the other rules)");
        }
        cfg.rules = select_rules(cfg.rules, &cli.rules)?;
    }
    match command {
        Command::Open(_) => unreachable!("resolved above"),
        Command::Tui { dir } => tui::run(cfg, config_path, dir, history::default_path()),
        Command::Run {
            paths,
            recursive,
            ext,
            apply,
        } => {
            let exts: Vec<String> = ext
                .iter()
                .map(|e| e.trim_start_matches('.').to_lowercase())
                .collect();
            run(&cfg, &paths, recursive, &exts, apply)
        }
        Command::Undo => {
            match history::undo_last(&history::default_path())? {
                None => println!("nothing to undo"),
                Some((n, skipped)) => {
                    println!("restored {n} file(s)");
                    skipped.iter().for_each(|s| eprintln!("skipped: {s}"));
                }
            }
            Ok(())
        }
        Command::Test { name } => {
            let compiled = compile_all(&cfg);
            match compiled.iter().enumerate().find_map(|(i, c)| match c {
                Ok(c) => c.apply(&name).map(|n| (i, n)),
                Err(_) => None,
            }) {
                Some((i, new)) => println!("{name}\n-> {new}   [{}]", cfg.rules[i].name),
                None => println!("no rule matched"),
            }
            Ok(())
        }
        Command::Rules => {
            for (i, (r, c)) in cfg.rules.iter().zip(compile_all(&cfg)).enumerate() {
                let err = c.err().map(|e| format!("  ERROR {e}")).unwrap_or_default();
                println!(
                    "{}. {}{err}\n   pattern:  {}\n   template: {}",
                    i + 1,
                    r.name,
                    r.pattern,
                    r.template
                );
            }
            Ok(())
        }
    }
}

/// Keeps the named rules (case-insensitive), in config order.
fn select_rules(rules: Vec<config::Rule>, names: &[String]) -> Result<Vec<config::Rule>> {
    for n in names {
        if !rules.iter().any(|r| r.name.eq_ignore_ascii_case(n)) {
            let known: Vec<_> = rules.iter().map(|r| r.name.as_str()).collect();
            bail!("no rule named '{n}' (available: {})", known.join(", "));
        }
    }
    Ok(rules
        .into_iter()
        .filter(|r| names.iter().any(|n| r.name.eq_ignore_ascii_case(n)))
        .collect())
}

pub fn compile_all(cfg: &config::Config) -> Vec<Result<Compiled, String>> {
    cfg.rules.iter().map(Compiled::compile).collect()
}

fn run(
    cfg: &config::Config,
    paths: &[PathBuf],
    recursive: bool,
    exts: &[String],
    apply: bool,
) -> Result<()> {
    let rules = compile_all(cfg);
    for (r, c) in cfg.rules.iter().zip(&rules) {
        if let Err(e) = c {
            eprintln!("warning: rule '{}' ignored: {e}", r.name);
        }
    }
    let files = plan::collect(paths, recursive, exts);
    let entries = plan::plan(&files, &rules, &plan::existing(&files));
    for e in &entries {
        let old = e.path.display();
        match (&e.status, &e.target) {
            (Status::Ready, Some(t)) => println!("{old}\n  -> {}", t.display()),
            (Status::Conflict(why), Some(t)) => println!("{old}\n  !! {} ({why})", t.display()),
            _ => {}
        }
    }
    let ready = entries.iter().filter(|e| e.status == Status::Ready).count();
    let conflicts = entries
        .iter()
        .filter(|e| matches!(e.status, Status::Conflict(_)))
        .count();
    let skipped = entries.len() - ready - conflicts;
    if !apply {
        println!(
            "\n{ready} to rename, {conflicts} conflict(s), {skipped} unmatched/unchanged (dry run, pass --apply)"
        );
        return Ok(());
    }
    let (done, errors) = plan::apply(&entries);
    let n = done.len();
    history::record(&history::default_path(), done)?;
    errors.iter().for_each(|e| eprintln!("error: {e}"));
    println!("\nrenamed {n} file(s), {conflicts} conflict(s) skipped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("formatui").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn rules_option_works_before_or_after_the_subcommand() {
        for args in [["--rules=tv", "run", "."], ["run", ".", "--rules=tv"]] {
            let cli = parse(&args);
            assert_eq!(cli.rules, ["tv"]);
            assert!(matches!(cli.command, Some(Command::Run { .. })), "{args:?}");
        }
    }

    #[test]
    fn a_bare_directory_opens_the_ui() {
        assert!(matches!(parse(&["some/dir"]).command, Some(Command::Open(a)) if a.len() == 1));
        assert!(parse(&[]).command.is_none());
    }
}
