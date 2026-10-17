//! Completion candidates are selected by argument position, never globally.
use crate::model::Config;
use serde::Serialize;

pub const ACTIONS: &[&str] = &["add", "edit", "list", "remove"];
pub const OPTIONS: &[&str] = &[
    "--help",
    "--version",
    "--help-all",
    "--edit-yaml",
    "--search",
    "--show",
    "--print",
    "--copy",
    "--move",
    "--rename-category",
    "--remove-category",
    "--undo",
    "--config-path",
    "--completions",
];
pub const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Context {
    None,
    Root,
    Categories,
    ActionCategory,
    Commands(String),
    Items(String),
    Shells,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub value: String,
    pub description: String,
}

pub fn context(words: &[String], position: usize) -> Context {
    if position == 0 {
        return Context::Root;
    }
    let first = words.first().map(String::as_str).unwrap_or("");
    match (first, position) {
        ("add" | "edit" | "remove", 1) => Context::ActionCategory,
        ("--completions", 1) => Context::Shells,
        (
            "--show" | "--print" | "--copy" | "--move" | "--rename-category" | "--remove-category"
            | "--" | "--edit-yaml",
            1,
        ) => Context::Categories,
        ("--rename-category", 2) | ("--move", 3) => Context::Categories,
        ("add", 2) => Context::Commands(words.get(1).cloned().unwrap_or_default()),
        ("edit" | "remove" | "--show" | "--print" | "--copy" | "--move", 2) => {
            Context::Items(words.get(1).cloned().unwrap_or_default())
        }
        _ => Context::None,
    }
}

pub fn candidates(data: &Config, context: &Context, prefix: &str) -> Vec<Candidate> {
    let plain = |value: &str, description: &str| Candidate {
        value: value.into(),
        description: description.into(),
    };
    let mut values = match context {
        Context::None => Vec::new(),
        Context::Shells => SHELLS.iter().map(|s| plain(s, "Shell")).collect(),
        Context::Categories | Context::ActionCategory => {
            let mut values: Vec<_> = data
                .iter()
                .map(|(name, entries)| plain(name, &format!("{} commands", entries.len())))
                .collect();
            if matches!(context, Context::ActionCategory) && prefix.starts_with('-') {
                values.extend([plain("--help", "Action help"), plain("-h", "Action help")]);
            }
            values
        }
        Context::Root => ACTIONS
            .iter()
            .map(|s| plain(s, "Action"))
            .chain(
                OPTIONS
                    .iter()
                    .filter(|_| prefix.starts_with('-'))
                    .map(|s| plain(s, "Option")),
            )
            .chain(
                data.iter()
                    .filter(|(name, _)| !name.starts_with('-'))
                    .map(|(name, entries)| plain(name, &format!("{} commands", entries.len()))),
            )
            .collect(),
        Context::Commands(category) | Context::Items(category) => {
            let mut values = Vec::new();
            let entries = data.get(category).map(Vec::as_slice).unwrap_or_default();
            let mut counts = std::collections::HashMap::new();
            for entry in entries {
                *counts.entry(entry.command.as_str()).or_insert(0usize) += 1;
            }
            for (i, entry) in entries.iter().enumerate() {
                // Numeric selectors and duplicated commands need an explicit item number.
                let selector = if matches!(context, Context::Items(_))
                    && (entry
                        .command
                        .strip_prefix('#')
                        .unwrap_or(&entry.command)
                        .parse::<usize>()
                        .is_ok()
                        || counts[entry.command.as_str()] > 1)
                {
                    format!("#{}", i + 1)
                } else {
                    entry.command.clone()
                };
                let description = if selector == entry.command {
                    format!("#{} · {}", i + 1, entry.description)
                } else {
                    format!("#{} · {} · {}", i + 1, entry.command, entry.description)
                };
                values.push(plain(&selector, &description));
                if matches!(context, Context::Items(_)) {
                    values.push(plain(&(i + 1).to_string(), &entry.command));
                }
            }
            values
        }
    };
    values.retain(|item| {
        item.value.starts_with(prefix)
            || matches!(context, Context::Items(category) if item.value.strip_prefix('#')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| data.get(category)?.get(i))
                .is_some_and(|entry| entry.command.starts_with(prefix)))
    });
    let mut seen = std::collections::HashSet::new();
    values.retain(|item| seen.insert(item.value.clone()));
    values
}

pub fn shell_quote(value: &str, shell: &str) -> String {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_-./:@+".contains(&b))
        && !value.is_empty()
    {
        return value.into();
    }
    if shell == "powershell" {
        return format!("'{}'", value.replace('\'', "''"));
    }
    if shell == "fish" {
        return value
            .chars()
            .map(|c| match c {
                '\n' => "\\n".into(),
                '\t' => "\\t".into(),
                '\r' => "\\r".into(),
                c if c.is_alphanumeric() || "_-./:@+".contains(c) => c.to_string(),
                c => format!("\\{c}"),
            })
            .collect();
    }
    let escaped: String = value
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".into(),
            '\'' => "\\'".into(),
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            c if c.is_control() => format!("\\x{:02x}", c as u32),
            c => c.to_string(),
        })
        .collect();
    format!("$'{escaped}'")
}

pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(include_str!("../completions/cmdnavi.bash")),
        "zsh" => Some(include_str!("../completions/_cmdnavi")),
        "fish" => Some(include_str!("../completions/cmdnavi.fish")),
        "powershell" => Some(include_str!("../completions/cmdnavi.ps1")),
        _ => None,
    }
}

/// Decode only shell quoting, including an unfinished quoted prefix. Never evaluate expansions.
pub fn bash_word(word: &str) -> String {
    let mut out = String::new();
    let mut chars = word.chars().peekable();
    let mut quote = '\0';
    while let Some(c) = chars.next() {
        match (quote, c) {
            ('\0', '$') if chars.peek() == Some(&'\'') => {
                chars.next();
                quote = 'a';
            }
            ('\0', '\'') => quote = '\'',
            ('\0', '"') => quote = '"',
            ('\0', '\\') => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            ('\'', '\'') | ('a', '\'') | ('"', '"') => quote = '\0',
            ('a', '\\') => {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'x' => {
                            let hex: String = (0..2)
                                .filter_map(|_| {
                                    if chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                                        chars.next()
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if let Ok(value) = u32::from_str_radix(&hex, 16)
                                && let Some(c) = char::from_u32(value)
                            {
                                out.push(c);
                            }
                        }
                        other => out.push(other),
                    }
                }
            }
            ('"', '\\') => {
                if chars
                    .peek()
                    .is_some_and(|c| matches!(c, '$' | '`' | '"' | '\\'))
                {
                    out.push(chars.next().unwrap());
                } else {
                    out.push('\\');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Bash Readline preserves an opening quote and closes it after insertion.
pub fn bash_insertion(value: &str, raw_prefix: &str) -> String {
    if raw_prefix.starts_with('"') {
        let mut escaped: String = value
            .chars()
            .map(|c| {
                if matches!(c, '$' | '`' | '"' | '\\') {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect();
        escaped.push('"');
        return escaped;
    }
    if raw_prefix.starts_with("$'") {
        let quoted = shell_quote(value, "bash");
        if let Some(stripped) = quoted.strip_prefix("$'") {
            return stripped.into();
        }
        return format!("{quoted}'");
    }
    if raw_prefix.starts_with('\'') {
        return format!("{}'", value.replace('\'', "'\\''"));
    }
    shell_quote(value, "bash")
}
