use crate::config;
use crate::editor::{self, ExternalEditor};
use crate::model::{CommandEntry, validate_category};
use crate::{Error, Result};
use std::io::{BufRead, Write};
use std::path::Path;

pub const HELP: &str = "cmdnavi - manage your personal command reference

Usage:
  cmdnavi
  cmdnavi <category>
  cmdnavi add [category] [\"command\"] [\"description\"]
  cmdnavi edit [category] [item-number]
  cmdnavi list
  cmdnavi remove <category>

Commands:
  add       Add a command
  edit      Edit registered commands
  list      List categories
  remove    Remove a command

Options:
  --browse        Browse, filter, copy and edit interactively
  --search <text>  Search categories, commands and descriptions
  -h, --help       Show help
  -v, --version    Show version

Examples:
  cmdnavi add network
  cmdnavi network
  cmdnavi --search \"IP address\"

Saved commands are displayed, never executed.
";

pub fn run(
    args: &[String],
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    match args {
        [] => overview(path, output)?,
        [one] => match one.as_str() {
            "-h" | "--help" | "help" => write!(output, "{HELP}")?,
            "-v" | "--version" | "version" => {
                writeln!(output, "cmdnavi {}", env!("CARGO_PKG_VERSION"))?
            }
            "list" => list(path, output)?,
            "--search" => {
                return Err(Error(
                    "search needs some text.\nTry `cmdnavi --search docker`.".into(),
                ));
            }
            "edit" => editor::edit_all_interactive(path, &ExternalEditor, input, output)?,
            "add" => {
                if let Some(category) = crate::interaction::choose_category(path, input, output)? {
                    add(path, &category, input, output)?;
                }
            }
            "remove" => return Err(missing_category(one)),
            option if option.starts_with('-') => {
                return Err(Error(format!(
                    "unknown option \"{option}\".\nRun `cmdnavi --help` and choose one of the supported options.\n\n{HELP}"
                )));
            }
            category => show(path, category, output)?,
        },
        [action, category, command] if action == "add" => crate::interaction::save_entry(
            path,
            category,
            CommandEntry {
                command: command.clone(),
                description: String::new(),
            },
            input,
            output,
        )?,
        [action, category, command, description] if action == "add" => {
            crate::interaction::save_entry(
                path,
                category,
                CommandEntry {
                    command: command.clone(),
                    description: description.clone(),
                },
                input,
                output,
            )?
        }
        [action, category, number] if action == "edit" => {
            checked_category(category)?;
            let data = config::load(path)?;
            let entries = data.get(category).ok_or_else(|| {
                Error(format!(
                    "Category \"{category}\" does not exist. Run `cmdnavi list`."
                ))
            })?;
            let index = number
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .ok_or_else(|| Error("Choose an item number starting at 1.".into()))?;
            crate::interaction::edit_entry(path, category, index, entries, input, output)?;
        }
        [command, query] if command == "--search" => search(path, query, output)?,
        [command, option]
            if matches!(command.as_str(), "add" | "edit" | "remove" | "list")
                && matches!(option.as_str(), "--help" | "-h") =>
        {
            write!(output, "{HELP}")?
        }
        [command, category] if command == "add" => add(path, category, input, output)?,
        [command, category] if command == "edit" => {
            checked_category(category)?;
            editor::edit_category_interactive(path, category, &ExternalEditor, input, output)?;
        }
        [command, category] if command == "remove" => remove(path, category, input, output)?,
        [command, ..] if matches!(command.as_str(), "list" | "help" | "version") => {
            return Err(Error(format!(
                "command \"{command}\" does not accept arguments\n\n{HELP}"
            )));
        }
        _ => {
            return Err(Error(format!(
                "the command could not be understood.\nPlease use one of the forms shown below.\n\n{HELP}"
            )));
        }
    }
    Ok(())
}

fn missing_category(command: &str) -> Error {
    Error(format!(
        "`{command}` needs a category.\nAdd a category name as shown below.\n\nUsage:\n  cmdnavi {command} <category>"
    ))
}

fn checked_category(category: &str) -> Result<()> {
    validate_category(category).map_err(Error)
}

pub fn show(path: &Path, category: &str, output: &mut dyn Write) -> Result<()> {
    checked_category(category)?;
    let config = config::load(path)?;
    let entries = config
        .get(category)
        .ok_or_else(|| {
            Error(format!(
                "category \"{category}\" does not exist.\nRun `cmdnavi list` to see your categories. To create it, use `cmdnavi add {category}` or `cmdnavi edit {category}`."
            ))
        })?;
    if entries.is_empty() {
        writeln!(
            output,
            "No commands in \"{category}\" yet.\nAdd one with `cmdnavi add {category}`."
        )?;
    }
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            writeln!(output)?;
        }
        write_entry(output, entry)?;
    }
    Ok(())
}

fn write_entry(output: &mut dyn Write, entry: &CommandEntry) -> Result<()> {
    writeln!(output, "{}", entry.command)?;
    for line in entry.description.lines() {
        writeln!(output, "  {line}")?;
    }
    Ok(())
}

pub fn overview(path: &Path, output: &mut dyn Write) -> Result<()> {
    let data = config::load(path)?;
    writeln!(output, "cmdnavi — your command reference\n")?;
    if data.is_empty() {
        writeln!(
            output,
            "No commands saved yet.\n\nGet started:  cmdnavi add network"
        )?;
    } else {
        let total: usize = data.values().map(Vec::len).sum();
        writeln!(output, "{} categories · {} commands\n", data.len(), total)?;
        let width = data.keys().map(String::len).max().unwrap_or(0);
        for (category, entries) in &data {
            writeln!(output, "  {category:width$}  {} commands", entries.len())?;
        }
        writeln!(
            output,
            "\nView:        cmdnavi <category>\nAdd:         cmdnavi add <category>"
        )?;
    }
    writeln!(
        output,
        "Browse:      cmdnavi --browse\nSearch:      cmdnavi --search <text>\nHelp:        cmdnavi --help"
    )?;
    Ok(())
}

pub fn search(path: &Path, query: &str, output: &mut dyn Write) -> Result<()> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error(
            "search text is empty.\nTry `cmdnavi --search docker`.".into(),
        ));
    }
    let needle = query.to_lowercase();
    let data = config::load(path)?;
    let mut count = 0;
    for (category, entries) in &data {
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| {
                category.to_lowercase().contains(&needle)
                    || entry.command.to_lowercase().contains(&needle)
                    || entry.description.to_lowercase().contains(&needle)
            })
            .collect();
        if matches.is_empty() {
            continue;
        }
        writeln!(output, "[{category}]\n")?;
        for entry in matches {
            write_entry(output, entry)?;
            writeln!(output)?;
            count += 1;
        }
    }
    if count == 0 {
        writeln!(
            output,
            "No matches for \"{query}\".\nTry a shorter keyword or run `cmdnavi list` to see your categories."
        )?;
    } else {
        writeln!(output, "{count} matching commands.")?;
    }
    Ok(())
}

pub fn list(path: &Path, output: &mut dyn Write) -> Result<()> {
    for category in config::load(path)?.keys() {
        writeln!(output, "{category}")?;
    }
    Ok(())
}

pub fn add(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    crate::interaction::add(path, category, input, output)
}

pub fn remove(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    checked_category(category)?;
    let data = config::load(path)?;
    let entries = data
        .get(category)
        .ok_or_else(|| {
            Error(format!(
                "nothing was removed because category \"{category}\" does not exist.\nRun `cmdnavi list` and choose one of your existing categories."
            ))
        })?;
    if entries.is_empty() {
        return Err(Error(format!(
            "category \"{category}\" has no commands to remove.\nAdd one with `cmdnavi add {category}` first."
        )));
    }
    for (index, entry) in entries.iter().enumerate() {
        writeln!(
            output,
            "[{}] {}\n    {}\n",
            index + 1,
            entry.command,
            entry.description
        )?;
    }
    let selection = loop {
        write!(
            output,
            "Select item to remove (1-{}, Enter or q to cancel): ",
            entries.len()
        )?;
        output.flush()?;
        let selection = read_line(input)?;
        let selection = selection.trim();
        if selection.is_empty() || selection.eq_ignore_ascii_case("q") {
            writeln!(output, "Cancelled.")?;
            return Ok(());
        }
        match selection.parse::<usize>() {
            Ok(number) if (1..=entries.len()).contains(&number) => break number,
            _ => writeln!(
                output,
                "Choose a number between 1 and {}, or Enter to cancel.",
                entries.len()
            )?,
        }
    };
    let command = entries[selection - 1].command.clone();
    write!(output, "Remove \"{command}\"? [y/N]: ")?;
    output.flush()?;
    let confirmation = read_line(input)?;
    if !matches!(
        confirmation.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ) {
        writeln!(output, "Cancelled.")?;
        return Ok(());
    }
    let original_entries = data[category].clone();
    config::update(path, |current| {
        if current.get(category) != Some(&original_entries) {
            return Err(Error(format!(
                "nothing was removed because category \"{category}\" changed while you were choosing an item.\nRun `cmdnavi remove {category}` again and choose from the updated list."
            )));
        }
        current
            .get_mut(category)
            .expect("category was checked")
            .remove(selection - 1);
        Ok(())
    })?;
    writeln!(output, "Removed \"{command}\" from \"{category}\".")?;
    Ok(())
}

pub(crate) fn read_line(input: &mut dyn BufRead) -> Result<String> {
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        return Err(Error(
            "the operation stopped because an input value is missing.\nProvide every requested value, or run the command interactively."
                .into(),
        ));
    }
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn add_show_list_and_remove_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut output = Vec::new();
        add(
            &path,
            "network",
            &mut Cursor::new("ip addr\nshow IP\n"),
            &mut output,
        )
        .unwrap();
        add(
            &path,
            "network",
            &mut Cursor::new("ss -tulpn\nshow ports\n"),
            &mut output,
        )
        .unwrap();
        let data = config::load(&path).unwrap();
        assert_eq!(data["network"].len(), 2);
        let mut shown = Vec::new();
        show(&path, "network", &mut shown).unwrap();
        assert!(
            String::from_utf8(shown)
                .unwrap()
                .contains("ss -tulpn\n  show ports")
        );
        let mut listed = Vec::new();
        list(&path, &mut listed).unwrap();
        assert_eq!(listed, b"network\n");
        remove(&path, "network", &mut Cursor::new("1\ny\n"), &mut output).unwrap();
        assert_eq!(
            config::load(&path).unwrap()["network"],
            vec![CommandEntry {
                command: "ss -tulpn".into(),
                description: "show ports".into()
            }]
        );
    }

    #[test]
    fn add_to_empty_config_creates_category() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        add(
            &path,
            "docker",
            &mut Cursor::new("docker ps\ncontainers\n"),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(config::load(&path).unwrap().contains_key("docker"));
    }

    #[test]
    fn argument_errors_are_specific() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut input = Cursor::new("");

        let missing = run(&["remove".into()], &path, &mut input, &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(missing.contains("needs a category"));

        let unknown = run(&["--unknown".into()], &path, &mut input, &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(unknown.contains("unknown option"));

        let extra = run(
            &["list".into(), "extra".into()],
            &path,
            &mut input,
            &mut Vec::new(),
        )
        .unwrap_err()
        .to_string();
        assert!(extra.contains("does not accept arguments"));
    }
}
