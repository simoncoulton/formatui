use regex::Regex;

use crate::config::Rule;

#[derive(Clone, Copy, Debug)]
enum Step {
    StripBrackets,
    DotsToSpaces,
    UnderscoresToSpaces,
    CollapseSpaces,
}

#[derive(Clone, Copy, Debug)]
enum Filter {
    Upper,
    Lower,
    Title,
    Trim,
    Pad2,
}

#[derive(Debug)]
enum Part {
    Lit(String),
    Field { name: String, filters: Vec<Filter> },
}

#[derive(Debug)]
pub struct Compiled {
    pub name: String,
    re: Regex,
    steps: Vec<Step>,
    exts: Vec<String>,
    parts: Vec<Part>,
}

impl Compiled {
    pub fn compile(rule: &Rule) -> Result<Self, String> {
        let re = Regex::new(&rule.pattern)
            .map_err(|e| format!("pattern: {}", first_line(&e.to_string())))?;
        let steps = rule
            .steps
            .iter()
            .map(|s| parse_step(s))
            .collect::<Result<Vec<_>, _>>()?;
        let parts = parse_template(&rule.template)?;
        let groups: Vec<&str> = re.capture_names().flatten().collect();
        for part in &parts {
            if let Part::Field { name, .. } = part
                && !groups.contains(&name.as_str())
            {
                return Err(format!("template: no group named '{name}' in pattern"));
            }
        }
        let exts = rule
            .extensions
            .iter()
            .map(|e| e.trim().trim_start_matches('.').to_lowercase())
            .collect();
        Ok(Self {
            name: rule.name.clone(),
            re,
            steps,
            exts,
            parts,
        })
    }

    /// Returns the new file name (extension preserved) if the rule matches.
    pub fn apply(&self, filename: &str) -> Option<String> {
        let (stem, ext) = split_ext(filename);
        if !self.exts.is_empty() && !self.exts.contains(&ext.to_lowercase()) {
            return None;
        }
        let mut stem = stem.to_string();
        for step in &self.steps {
            stem = run_step(*step, &stem);
        }
        let caps = self.re.captures(&stem)?;
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Lit(s) => out.push_str(s),
                Part::Field { name, filters } => {
                    let mut v = caps.name(name).map_or("", |m| m.as_str()).to_string();
                    for f in filters {
                        v = run_filter(*f, &v);
                    }
                    out.push_str(&v);
                }
            }
        }
        let out = collapse(&out.replace(['/', '\0'], "-"));
        if out.is_empty() {
            return None;
        }
        Some(if ext.is_empty() {
            out
        } else {
            format!("{out}.{ext}")
        })
    }
}

fn first_line(s: &str) -> String {
    s.lines().last().unwrap_or(s).trim().to_string()
}

pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    }
}

fn parse_step(s: &str) -> Result<Step, String> {
    Ok(match s.trim() {
        "strip_brackets" => Step::StripBrackets,
        "dots_to_spaces" => Step::DotsToSpaces,
        "underscores_to_spaces" => Step::UnderscoresToSpaces,
        "collapse_spaces" => Step::CollapseSpaces,
        other => return Err(format!("steps: unknown step '{other}'")),
    })
}

fn run_step(step: Step, s: &str) -> String {
    match step {
        Step::StripBrackets => {
            let mut out = String::new();
            let mut depth = 0u32;
            for c in s.chars() {
                match c {
                    '[' => depth += 1,
                    ']' if depth > 0 => depth -= 1,
                    _ if depth == 0 => out.push(c),
                    _ => {}
                }
            }
            out
        }
        Step::DotsToSpaces => s.replace('.', " "),
        Step::UnderscoresToSpaces => s.replace('_', " "),
        Step::CollapseSpaces => collapse(s),
    }
}

fn parse_filter(s: &str) -> Result<Filter, String> {
    Ok(match s.trim() {
        "upper" => Filter::Upper,
        "lower" => Filter::Lower,
        "title" => Filter::Title,
        "trim" => Filter::Trim,
        "pad2" => Filter::Pad2,
        other => return Err(format!("template: unknown filter '{other}'")),
    })
}

fn run_filter(f: Filter, s: &str) -> String {
    match f {
        Filter::Upper => s.to_uppercase(),
        Filter::Lower => s.to_lowercase(),
        Filter::Trim => s.trim().to_string(),
        Filter::Pad2 => format!("{:0>2}", s),
        Filter::Title => s
            .split(' ')
            .map(|w| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn parse_template(t: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            parts.push(Part::Lit(rest[..open].to_string()));
        }
        let close = rest[open..].find('}').ok_or("template: unclosed '{'")? + open;
        let mut it = rest[open + 1..close].split('|');
        let name = it.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            return Err("template: empty '{}'".into());
        }
        let filters = it.map(parse_filter).collect::<Result<Vec<_>, _>>()?;
        parts.push(Part::Field { name, filters });
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(Part::Lit(rest.to_string()));
    }
    Ok(parts)
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tv() -> Compiled {
        Compiled::compile(&Rule {
            name: "tv".into(),
            pattern:
                r"(?i)^(?P<title>.+?)(?: (?:19|20)\d{2})?(?: -)? S(?P<season>\d+)E(?P<episode>\d+)"
                    .into(),
            template: "{title} - S{season|pad2}E{episode|pad2}".into(),
            steps: vec!["strip_brackets".into(), "dots_to_spaces".into()],
            extensions: vec!["mkv".into()],
        })
        .unwrap()
    }

    #[test]
    fn renames_release_name() {
        let n = "Example.Show.2024.S02E06.1080p.HEVC.x265-GROUP[site.to].mkv";
        assert_eq!(tv().apply(n).as_deref(), Some("Example Show - S02E06.mkv"));
    }

    #[test]
    fn already_formatted_is_stable() {
        assert_eq!(
            tv().apply("Example Show - S02E06.mkv").as_deref(),
            Some("Example Show - S02E06.mkv")
        );
    }

    #[test]
    fn no_match_and_bad_group() {
        assert!(tv().apply("holiday.jpg").is_none());
        assert!(tv().apply("Example.Show.S02E06.avi").is_none());
        let bad = Rule {
            name: "x".into(),
            pattern: "(?P<a>x)".into(),
            template: "{b}".into(),
            steps: vec![],
            extensions: vec![],
        };
        assert!(Compiled::compile(&bad).is_err());
    }
}
