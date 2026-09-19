use crate::config;
use crate::editor::{self, ExternalEditor};
use crate::model::{CommandEntry, validate_category};
use crate::{Error, Result};
use std::io::{BufRead, Write};
use std::path::Path;

pub const HELP: &str = "cmdnavi - manage your personal command reference

Usage:
  cmdnavi <category>
  cmdnavi add <category>
  cmdnavi edit [category]
  cmdnavi list
  cmdnavi remove <category>

Commands:
  add       Add a command
  edit      Edit registered commands
  list      List categories
  remove    Remove a command

Options:
  -h, --help       Show help
  -v, --version    Show version
";

pub fn run(
    args: &[String],
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    match args {
        [] => write!(output, "{HELP}")?,
        [one] => match one.as_str() {
            "-h" | "--help" | "help" => write!(output, "{HELP}")?,
            "-v" | "--version" | "version" => {
                writeln!(output, "cmdnavi {}", env!("CARGO_PKG_VERSION"))?
            }
            "list" => list(path, output)?,
            "edit" => editor::edit_all_interactive(path, &ExternalEditor, input, output)?,
            "add" | "remove" => return Err(missing_category(one)),
            option if option.starts_with('-') => {
                return Err(Error(format!(
                    "unknown option \"{option}\".\nRun `cmdnavi --help` and choose one of the supported options.\n\n{HELP}"
                )));
            }
            category => show(path, category, output)?,
        },
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
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            writeln!(output)?;
        }
        writeln!(output, "{}\n  {}", entry.command, entry.description)?;
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
    checked_category(category)?;
    write!(output, "Command: ")?;
    output.flush()?;
    let command = read_line(input)?;
    if command.trim().is_empty() {
        return Err(Error(
            "the command was not added because command is empty.\nEnter the command text you want to save and try again."
                .into(),
        ));
    }
    write!(output, "Description: ")?;
    output.flush()?;
    let description = read_line(input)?;
    if description.trim().is_empty() {
        return Err(Error(
            "the command was not added because description is empty.\nEnter a short explanation of what the command does and try again."
                .into(),
        ));
    }
    let created = config::update(path, |data| {
        let created = !data.contains_key(category);
        data.entry(category.into()).or_default().push(CommandEntry {
            command,
            description,
        });
        Ok(created)
    })?;
    if created {
        writeln!(
            output,
            "Category \"{category}\" does not exist.\nCreated category \"{category}\"."
        )?;
    }
    writeln!(output, "Added command to \"{category}\".")?;
    Ok(())
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
    write!(output, "Select item to remove: ")?;
    output.flush()?;
    let selection = read_line(input)?.trim().parse::<usize>().map_err(|_| {
            Error("nothing was removed because the selection is not a number.\nEnter one of the item numbers shown above.".into())
    })?;
    if selection == 0 || selection > entries.len() {
        return Err(Error(format!(
            "nothing was removed because {selection} is outside the available range (1-{}).\nEnter a number between 1 and {}.",
            entries.len(),
            entries.len()
        )));
    }
    let command = entries[selection - 1].command.clone();
    write!(output, "Remove \"{command}\"? [y/N]: ")?;
    output.flush()?;
    let confirmation = read_line(input)?;
    if !confirmation.eq_ignore_ascii_case("y") {
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

fn read_line(input: &mut dyn BufRead) -> Result<String> {
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

        let missing = run(&["add".into()], &path, &mut input, &mut Vec::new())
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
