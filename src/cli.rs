use crate::model::{CommandEntry, matches_query, validate_category};
use crate::session::Session;
use crate::{Error, Result, clipboard, completion, config, editor, interaction};
use std::io::{BufRead, Write};
use std::path::Path;

pub const HELP: &str = "cmdnavi - your personal command reference

Usage:
  cmdnavi                         Browse and search (↑↓ select, F1 help)
  cmdnavi <category>               Show numbered commands and descriptions
  cmdnavi add [category] [\"command\"] [\"description\"]
  cmdnavi edit [category] [item]    YAML editing, or edit one selected item
  cmdnavi remove <category> [item]  Select a command and confirm removal
  cmdnavi list                     List category names

Options:
  --show <category> [item]          Show a category or one item (also leading '-')
  --print <category> <item>         Output only the command text
  --copy <category> [item]          Copy a command, never execute it
  --move <category> <item> <target> Move a command to another category
  --rename-category <old> <new>     Rename a category
  --remove-category <category>     Confirm removal of a category and its commands
  --undo                          Undo last change; repeat to redo
  --config-path                   Show where commands are saved
  --browse                        Open the interactive browser
  --search <text>                  Search using all words, across all fields
  --completions <shell>            Shell setup: bash, zsh, fish, powershell
  -h, --help                      Show help (also: add --help, edit --help)
  -v, --version                   Show version

An item is a 1-based number or an exact saved command, quoted if it contains spaces.
Tab completion is context-specific; description/search fields have no command completion.
Interactive add: filter categories; /back edits the command, /category changes its category at the description prompt.
Category names accept Japanese and other letters, digits, '_' and '-'.
Use `cmdnavi --completions bash` to print the Bash integration; see README for setup.
";

pub fn run(
    args: &[String],
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    run_with_terminal(args, path, input, output, false)
}
pub fn run_with_terminal(
    args: &[String],
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    terminal: bool,
) -> Result<()> {
    dispatch(args, path, &mut Session::new(input, output, terminal))
}
/// Handle commands that do not read or write user configuration.
pub fn run_metadata(args: &[String], output: &mut dyn Write) -> Option<Result<()>> {
    let strings: Vec<_> = args.iter().map(String::as_str).collect();
    match strings.as_slice() {
        ["--completions", _] | ["-h" | "--help" | "help" | "-v" | "--version" | "version"] => {}
        [action, "-h" | "--help"] if completion::ACTIONS.contains(action) => {}
        _ => return None,
    }
    Some((|| {
        match strings.as_slice() {
            ["--completions", shell] => {
                let script = completion::script(shell)
                    .ok_or_else(|| Error("Choose bash, zsh, fish or powershell.".into()))?;
                write!(output, "{script}")?;
            }
            ["-h" | "--help" | "help"] => write!(output, "{HELP}")?,
            ["-v" | "--version" | "version"] => {
                writeln!(output, "cmdnavi {}", env!("CARGO_PKG_VERSION"))?
            }
            [action, "-h" | "--help"] if completion::ACTIONS.contains(action) => {
                action_help(action, output)?
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}

fn dispatch(args: &[String], path: &Path, s: &mut Session<'_>) -> Result<()> {
    if let Some(result) = run_metadata(args, s.output) {
        return result;
    }
    let strings: Vec<_> = args.iter().map(String::as_str).collect();
    match strings.as_slice() {
        [] => overview(path, s.output)?,
        ["--complete", shell, position, "--", ..] => {
            let position = position
                .parse::<usize>()
                .map_err(|_| Error("Invalid completion position.".into()))?;
            let mut words = args[4..].to_vec();
            if let Some(word) = words.get_mut(position) {
                *word = word
                    .strip_prefix("__CMDNAVI_PREFIX__")
                    .unwrap_or(word)
                    .to_owned();
            }
            let raw_prefix = words.get(position).cloned().unwrap_or_default();
            if *shell == "bash" {
                for word in &mut words {
                    *word = completion::bash_word(word);
                }
            }
            let prefix = words.get(position).map(String::as_str).unwrap_or("");
            let data = config::load(path).unwrap_or_default();
            let context = completion::context(&words, position);
            let mut values = completion::candidates(&data, &context, prefix);
            if *shell == "fish" {
                // Fish uses TAB as the candidate/description separator even with NUL records.
                // Reference existing items by index rather than insert a truncated command.
                values.retain_mut(|item| {
                    if !item.value.contains('\t') {
                        return true;
                    }
                    if let completion::Context::Items(category) = &context
                        && let Some(entries) = data.get(category)
                        && let Ok(index) = interaction::resolve_item(entries, &item.value)
                    {
                        item.description = format!(
                            "{} · {}",
                            crate::session::visible(&item.value),
                            item.description
                        );
                        item.value = format!("#{}", index + 1);
                        return true;
                    }
                    false
                });
            }
            if *shell == "powershell" {
                let values: Vec<_> = values.iter().map(|c| serde_json::json!({"value":c.value,"description":crate::session::visible(&c.description),"insert":completion::shell_quote(&c.value,shell)})).collect();
                writeln!(
                    s.output,
                    "{}",
                    serde_json::to_string(&values).map_err(|e| Error(e.to_string()))?
                )?;
            } else {
                for item in values {
                    let value = if *shell == "bash" {
                        completion::bash_insertion(&item.value, &raw_prefix)
                    } else {
                        completion::shell_quote(&item.value, shell)
                    };
                    if *shell == "zsh" {
                        write!(s.output, "{}\0", item.value)?;
                    } else if *shell == "bash" {
                        write!(s.output, "{value}\0")?;
                    } else if *shell == "fish" {
                        write!(
                            s.output,
                            "{}\t{}\0",
                            item.value,
                            crate::session::visible(&item.description)
                        )?;
                    } else {
                        writeln!(s.output, "{value}")?;
                    }
                }
            }
        }
        ["--config-path"] => writeln!(s.output, "{}", path.display())?,
        ["--undo"] => {
            config::undo(path)?;
            writeln!(
                s.output,
                "Last change undone. Repeat `cmdnavi --undo` to redo."
            )?;
        }
        ["list"] => list(path, s.output)?,
        ["add"] => {
            if let Some(category) = interaction::choose_category_with(path, s)? {
                interaction::add_with(path, &category, s)?;
            }
        }
        ["add", category] => interaction::add_with(path, category, s)?,
        ["add", category, command] => interaction::save_entry_with(
            path,
            category,
            CommandEntry {
                command: (*command).into(),
                description: String::new(),
            },
            false,
            s,
        )?,
        ["add", category, command, description] => interaction::save_entry_with(
            path,
            category,
            CommandEntry {
                command: (*command).into(),
                description: (*description).into(),
            },
            true,
            s,
        )?,
        ["edit"] => editor::edit_all_interactive(path, &editor::ExternalEditor, s.input, s.output)?,
        ["edit", category] => {
            validate_category(category).map_err(Error)?;
            editor::edit_category_interactive(
                path,
                category,
                &editor::ExternalEditor,
                s.input,
                s.output,
            )?;
        }
        ["edit", category, item] => {
            let entries = entries(path, category)?;
            let i = interaction::resolve_item(&entries, item)?;
            interaction::edit_entry_with(path, category, i, &entries, s)?;
        }
        ["remove" | "--copy", category] => {
            let entries = entries(path, category)?;
            if let Some(i) = interaction::pick_entry(category, &entries, s)? {
                if strings[0] == "remove" {
                    interaction::remove_entry(path, category, i, &entries, s)?;
                } else {
                    clipboard::copy(&entries[i].command)?;
                    writeln!(s.output, "Copied command from [{category}].")?;
                }
            }
        }
        ["remove" | "--copy", category, item] => {
            let entries = entries(path, category)?;
            let i = interaction::resolve_item(&entries, item)?;
            if strings[0] == "remove" {
                interaction::remove_entry(path, category, i, &entries, s)?;
            } else {
                clipboard::copy(&entries[i].command)?;
                writeln!(s.output, "Copied command from [{category}].")?;
            }
        }
        ["--move", source, item, target] => {
            let entries = entries(path, source)?;
            let i = interaction::resolve_item(&entries, item)?;
            interaction::move_entry(path, source, i, &entries, target)?;
            writeln!(s.output, "Moved command to [{target}].")?;
        }
        ["--rename-category", old, new] => {
            let entries = entries(path, old)?;
            rename_category(path, old, new, &entries)?;
            writeln!(s.output, "Renamed [{old}] to [{new}].")?;
        }
        ["--remove-category", category] => {
            let entries = entries(path, category)?;
            if s.confirm(&format!(
                "Remove [{category}] and its {} commands? [y/N]: ",
                entries.len()
            ))? {
                config::update(path, |data| {
                    if data.get(*category) != Some(&entries) {
                        return Err(interaction::conflict());
                    }
                    data.remove(*category);
                    Ok(())
                })?;
                writeln!(
                    s.output,
                    "Removed category. Use `cmdnavi --undo` to restore it."
                )?;
            } else {
                writeln!(s.output, "Cancelled.")?;
            }
        }
        ["--search", query] => search(path, query, s.output)?,
        ["--" | "--show", category] => show(path, category, s.output)?,
        ["--show" | "--print", category, item] => {
            let entries = entries(path, category)?;
            let i = interaction::resolve_item(&entries, item)?;
            if strings[0] == "--print" {
                write!(s.output, "{}", entries[i].command)?;
            } else {
                write_entry(s.output, &entries[i])?;
            }
        }
        ["remove" | "--copy" | "--show" | "--print"] => {
            return Err(Error(format!(
                "`{}` needs a category.\nTry `cmdnavi {} network` or `cmdnavi --help`.",
                strings[0], strings[0]
            )));
        }
        [option] if option.starts_with('-') => {
            return Err(Error(format!(
                "unknown option \"{option}\".\nRun `cmdnavi --help` for supported options."
            )));
        }
        [category] => show(path, category, s.output)?,
        ["list" | "help" | "version", ..] => {
            return Err(Error(
                "This command does not accept arguments. Run `cmdnavi --help`.".into(),
            ));
        }
        _ => {
            return Err(Error(format!(
                "The command could not be understood.\n{HELP}"
            )));
        }
    }
    Ok(())
}

fn action_help(action: &str, output: &mut dyn Write) -> Result<()> {
    let text = match action {
        "add" => {
            "cmdnavi add [category] [\"command\"] [\"description\"]\nChoose/create a category; Tab completes categories only here. The command field completes saved commands in that category. Description has no completion. Enter skips the description; Ctrl+C or /cancel cancels.\nIn a terminal, filter categories by any part of the name, then use Up/Down and Enter. At the description prompt, /back edits the command and /category changes its category without losing the command.\nMultiline paste is supported in a terminal. In scripts, pass quoted arguments. Omitting description preserves an existing description."
        }
        "edit" => {
            "cmdnavi edit [category] [item]\nWithout an item, edit YAML using VISUAL/EDITOR. With a number or quoted saved command, edit that item with the current values prefilled. Tab completes items only after the category. Ctrl+C or /cancel cancels."
        }
        "remove" => {
            "cmdnavi remove <category> [item]\nChoose a number or exact saved command; Tab completes items after the category. Removal requires y/yes. Enter, q or Ctrl+C at selection cancels. `cmdnavi --undo` restores the last change."
        }
        _ => {
            "cmdnavi list\nPrint category names only, one per line, suitable for scripts. `cmdnavi` opens the browser; --config-path prints the data location."
        }
    };
    writeln!(output, "Usage:\n  {text}")?;
    Ok(())
}

pub fn entries(path: &Path, category: &str) -> Result<Vec<CommandEntry>> {
    validate_category(category).map_err(Error)?;
    config::load(path)?.remove(category).ok_or_else(|| Error(format!("Category \"{category}\" does not exist. Run `cmdnavi list` or `cmdnavi add {category}`.")))
}
pub fn rename_category(path: &Path, old: &str, new: &str, snapshot: &[CommandEntry]) -> Result<()> {
    validate_category(new).map_err(Error)?;
    if old == new {
        return Ok(());
    }
    config::update(path, |data| {
        if data.get(old).map(Vec::as_slice) != Some(snapshot) {
            return Err(interaction::conflict());
        }
        if data.contains_key(new) {
            return Err(Error(
                "That category already exists. Choose another name or move individual commands."
                    .into(),
            ));
        }
        let entries = data.remove(old).unwrap();
        data.insert(new.into(), entries);
        Ok(())
    })
}

pub fn show(path: &Path, category: &str, output: &mut dyn Write) -> Result<()> {
    let entries = entries(path, category)?;
    writeln!(output, "[{category}] · {} commands", entries.len())?;
    if entries.is_empty() {
        writeln!(output, "No commands yet. Use `cmdnavi add {category}`.")?;
    }
    for (i, entry) in entries.iter().enumerate() {
        writeln!(output, "\n[{}]", i + 1)?;
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
        writeln!(
            output,
            "{} categories · {} commands\n",
            data.len(),
            data.values().map(Vec::len).sum::<usize>()
        )?;
        let width = data.keys().map(String::len).max().unwrap_or(0);
        for (name, entries) in &data {
            writeln!(output, "  {name:width$}  {} commands", entries.len())?;
        }
    }
    writeln!(
        output,
        "\nView: cmdnavi --show <category>\nBrowse: cmdnavi --browse\nSearch: cmdnavi --search <text>\nHelp: cmdnavi --help\nSaved data: {}",
        path.display()
    )?;
    Ok(())
}
pub fn search(path: &Path, query: &str, output: &mut dyn Write) -> Result<()> {
    if query.trim().is_empty() {
        return Err(Error(
            "Search text is empty. Try `cmdnavi --search docker`.".into(),
        ));
    }
    let data = config::load(path)?;
    let mut count = 0;
    for (category, entries) in data {
        for (i, entry) in entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches_query(&category, e, query))
        {
            writeln!(output, "[{category}] #{}", i + 1)?;
            write_entry(output, entry)?;
            writeln!(output)?;
            count += 1;
        }
    }
    if count == 0 {
        writeln!(
            output,
            "No matches for \"{query}\". Try fewer words or `cmdnavi list`."
        )?;
    } else {
        writeln!(output, "{count} matching commands.")?;
    }
    Ok(())
}
pub fn list(path: &Path, output: &mut dyn Write) -> Result<()> {
    for name in config::load(path)?.keys() {
        writeln!(output, "{name}")?;
    }
    Ok(())
}
pub fn add(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    interaction::add(path, category, input, output)
}
pub fn remove(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    let entries = entries(path, category)?;
    let mut session = Session::new(input, output, false);
    if let Some(i) = interaction::pick_entry(category, &entries, &mut session)? {
        interaction::remove_entry(path, category, i, &entries, &mut session)?;
    }
    Ok(())
}
pub(crate) fn read_line(input: &mut dyn BufRead) -> Result<String> {
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        return Err(Error("The operation stopped because an input value is missing. Provide every requested value, or run interactively.".into()));
    }
    Ok(value.trim_end_matches(['\r', '\n']).into())
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
