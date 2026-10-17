use crate::completion::Context;
use crate::model::{CommandEntry, Config, validate_category};
use crate::session::Session;
use crate::{Error, Result, config};
use std::io::{BufRead, Write};
use std::path::Path;

pub fn choose_category(
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Option<String>> {
    choose_category_with(path, &mut Session::new(input, output, false))
}
pub fn add(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    add_with(path, category, &mut Session::new(input, output, false))
}
pub fn save_entry(
    path: &Path,
    category: &str,
    entry: CommandEntry,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    save_entry_with(
        path,
        category,
        entry,
        true,
        &mut Session::new(input, output, false),
    )
}
pub fn edit_entry(
    path: &Path,
    category: &str,
    index: usize,
    snapshot: &[CommandEntry],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    edit_entry_with(
        path,
        category,
        index,
        snapshot,
        &mut Session::new(input, output, false),
    )
}

pub fn choose_category_with(path: &Path, session: &mut Session<'_>) -> Result<Option<String>> {
    let data = config::load(path)?;
    let categories: Vec<_> = data.keys().collect();
    writeln!(
        session.output,
        "Choose a category or type a new name. Tab completes; ↑↓ recall candidates. Ctrl+C or /cancel cancels."
    )?;
    for (i, name) in categories.iter().enumerate() {
        writeln!(
            session.output,
            "  #{}  {} ({} commands)",
            i + 1,
            name,
            data[*name].len()
        )?;
    }
    loop {
        let Some(value) = session.ask(
            "Category (name / #number, Enter = general): ",
            "",
            Context::Categories,
            &data,
        )?
        else {
            return Ok(None);
        };
        let value = value.trim();
        let category = if let Some(index) = value.strip_prefix('#') {
            match index
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| categories.get(i))
            {
                Some(name) => name.as_str(),
                None => {
                    writeln!(
                        session.output,
                        "Choose one of the #numbers above, or type a name."
                    )?;
                    continue;
                }
            }
        } else if value.is_empty() {
            "general"
        } else {
            value
        };
        match validate_category(category) {
            Ok(()) => return Ok(Some(category.to_owned())),
            Err(message) => writeln!(session.output, "{message}")?,
        }
    }
}

pub fn add_with(path: &Path, category: &str, session: &mut Session<'_>) -> Result<()> {
    validate_category(category).map_err(Error)?;
    let data = config::load(path)?;
    writeln!(
        session.output,
        "Add to [{category}] · Tab completes saved commands · Ctrl+C or /cancel cancels"
    )?;
    let command = loop {
        let Some(value) =
            session.ask("Command: ", "", Context::Commands(category.into()), &data)?
        else {
            return Ok(());
        };
        if !value.trim().is_empty() {
            break value;
        }
        writeln!(session.output, "Enter a command to save.")?;
    };
    let Some(description) = session.plain("Description (optional, Enter to skip): ")? else {
        return Ok(());
    };
    session.finish_form()?;
    save_entry_with(
        path,
        category,
        CommandEntry {
            command,
            description: description.clone(),
        },
        !description.is_empty(),
        session,
    )
}

pub fn save_entry_with(
    path: &Path,
    category: &str,
    mut entry: CommandEntry,
    description_provided: bool,
    session: &mut Session<'_>,
) -> Result<()> {
    validate_category(category).map_err(Error)?;
    entry.validate().map_err(|e| Error(e.into()))?;
    let data = config::load(path)?;
    let original = data.get(category);
    let duplicate = original.and_then(|entries| {
        entries
            .iter()
            .position(|saved| saved.command.trim() == entry.command.trim())
    });
    if let Some(i) = duplicate {
        let saved = &original.unwrap()[i];
        if !description_provided {
            entry.description = saved.description.clone();
        }
        if entry.description == saved.description {
            writeln!(session.output, "Already saved in [{category}]. No changes.")?;
            return Ok(());
        }
        writeln!(
            session.output,
            "Already saved in [{category}]:\n  {}\n  {}",
            saved.command, saved.description
        )?;
        if !session.confirm("Replace its description? [y/N]: ")? {
            writeln!(session.output, "Cancelled.")?;
            return Ok(());
        }
    }
    config::update(path, |current| {
        if let Some(i) = duplicate {
            if current.get(category) != original {
                return Err(conflict());
            }
            current.get_mut(category).unwrap()[i].description = entry.description;
        } else {
            let entries = current.entry(category.into()).or_default();
            if entries
                .iter()
                .any(|saved| saved.command.trim() == entry.command.trim())
            {
                return Err(conflict());
            }
            entries.push(entry);
        }
        Ok(())
    })?;
    if original.is_none() {
        writeln!(session.output, "Created category \"{category}\".")?;
    }
    writeln!(
        session.output,
        "{} command in [{category}].",
        if duplicate.is_some() {
            "Updated"
        } else {
            "Saved"
        }
    )?;
    Ok(())
}

pub(crate) fn conflict() -> Error {
    Error("This category changed while you were editing. Nothing was overwritten. Please reload and try again.".into())
}

pub fn edit_entry_with(
    path: &Path,
    category: &str,
    index: usize,
    snapshot: &[CommandEntry],
    session: &mut Session<'_>,
) -> Result<()> {
    let old = snapshot
        .get(index)
        .ok_or_else(|| Error("Choose an existing item number or saved command.".into()))?;
    let data = Config::from([(category.into(), snapshot.to_vec())]);
    writeln!(
        session.output,
        "Edit [{category}] · Enter keeps current value · Ctrl+C or /cancel cancels"
    )?;
    if !session.terminal {
        writeln!(
            session.output,
            "Command: {}\nDescription: {}",
            old.command, old.description
        )?;
    }
    let command = loop {
        let initial = if session.terminal { &old.command } else { "" };
        let Some(value) = session.ask(
            "New command: ",
            initial,
            Context::Commands(category.into()),
            &data,
        )?
        else {
            return Ok(());
        };
        if value.is_empty() {
            break old.command.clone();
        }
        if !value.trim().is_empty() {
            break value;
        }
        writeln!(
            session.output,
            "Enter a command, or press Enter to keep the current one."
        )?;
    };
    let initial = if session.terminal {
        &old.description
    } else {
        ""
    };
    let Some(value) = session.ask(
        "New description (/clear to remove): ",
        initial,
        Context::None,
        &data,
    )?
    else {
        return Ok(());
    };
    let description = match value.as_str() {
        "/clear" => String::new(),
        "" if !session.terminal => old.description.clone(),
        _ => value,
    };
    let edited = CommandEntry {
        command,
        description,
    };
    edited.validate().map_err(|e| Error(e.into()))?;
    if edited == *old {
        writeln!(session.output, "No changes.")?;
        return Ok(());
    }
    config::update(path, |current| {
        if current.get(category).map(Vec::as_slice) != Some(snapshot) {
            return Err(conflict());
        }
        let entries = current.get_mut(category).unwrap();
        if entries
            .iter()
            .enumerate()
            .any(|(i, saved)| i != index && saved.command.trim() == edited.command.trim())
        {
            return Err(Error(
                "That command is already in this category. No changes saved.".into(),
            ));
        }
        entries[index] = edited;
        Ok(())
    })?;
    writeln!(session.output, "Updated command in [{category}].")?;
    Ok(())
}

pub fn resolve_item(entries: &[CommandEntry], selector: &str) -> Result<usize> {
    if let Ok(number) = selector
        .strip_prefix('#')
        .unwrap_or(selector)
        .parse::<usize>()
    {
        return number
            .checked_sub(1)
            .filter(|i| *i < entries.len())
            .ok_or_else(|| {
                Error(format!(
                    "Choose an item number between 1 and {}.",
                    entries.len()
                ))
            });
    }
    let indices: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.command == selector)
        .map(|(i, _)| i)
        .collect();
    if indices.len() == 1 {
        Ok(indices[0])
    } else {
        Err(Error("Choose an existing item number or the exact saved command. Use a number if commands are duplicated.".into()))
    }
}

pub fn pick_entry(
    category: &str,
    entries: &[CommandEntry],
    session: &mut Session<'_>,
) -> Result<Option<usize>> {
    if entries.is_empty() {
        return Err(Error(format!(
            "Category [{category}] has no commands. Use `cmdnavi add {category}`."
        )));
    }
    for (i, entry) in entries.iter().enumerate() {
        writeln!(
            session.output,
            "[{}] {}\n    {}",
            i + 1,
            crate::session::visible(&entry.command),
            crate::session::visible(&entry.description)
        )?;
    }
    let data = Config::from([(category.into(), entries.to_vec())]);
    loop {
        let Some(value) = session.ask(
            &format!(
                "Select item (1-{}, Tab completes, Enter or q cancels): ",
                entries.len()
            ),
            "",
            Context::Items(category.into()),
            &data,
        )?
        else {
            return Ok(None);
        };
        if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("q") {
            writeln!(session.output, "Cancelled.")?;
            return Ok(None);
        }
        match resolve_item(entries, &value) {
            Ok(index) => return Ok(Some(index)),
            Err(error) => writeln!(session.output, "{error}")?,
        }
    }
}

pub fn remove_entry(
    path: &Path,
    category: &str,
    index: usize,
    snapshot: &[CommandEntry],
    session: &mut Session<'_>,
) -> Result<()> {
    let entry = snapshot
        .get(index)
        .ok_or_else(|| Error("Choose an existing item.".into()))?;
    if !session.confirm(&format!(
        "Remove \"{}\"? [y/N]: ",
        crate::session::visible(&entry.command)
    ))? {
        writeln!(session.output, "Cancelled.")?;
        return Ok(());
    }
    config::update(path, |current| {
        if current.get(category).map(Vec::as_slice) != Some(snapshot) {
            return Err(conflict());
        }
        current.get_mut(category).unwrap().remove(index);
        Ok(())
    })?;
    writeln!(
        session.output,
        "Removed command from [{category}]. Use `cmdnavi --undo` to restore it."
    )?;
    Ok(())
}

pub fn move_entry(
    path: &Path,
    source: &str,
    index: usize,
    snapshot: &[CommandEntry],
    target: &str,
) -> Result<()> {
    validate_category(target).map_err(Error)?;
    if source == target {
        return Ok(());
    }
    let entry = snapshot
        .get(index)
        .ok_or_else(|| Error("Choose an existing item.".into()))?
        .clone();
    config::update(path, |data| {
        if data.get(source).map(Vec::as_slice) != Some(snapshot) {
            return Err(conflict());
        }
        if data.get(target).is_some_and(|entries| {
            entries
                .iter()
                .any(|e| e.command.trim() == entry.command.trim())
        }) {
            return Err(Error(
                "The target category already contains this command. Nothing moved.".into(),
            ));
        }
        data.get_mut(source).unwrap().remove(index);
        data.entry(target.into()).or_default().push(entry);
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn stale_item_edit_preserves_concurrent_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let original = vec![CommandEntry {
            command: "ip addr".into(),
            description: "old".into(),
        }];
        let newer = vec![CommandEntry {
            command: "ip link".into(),
            description: "concurrent".into(),
        }];
        config::save(
            &path,
            &crate::model::Config::from([("network".into(), newer.clone())]),
        )
        .unwrap();
        let error = edit_entry(
            &path,
            "network",
            0,
            &original,
            &mut Cursor::new("\nmy edit\n"),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Nothing was overwritten"));
        assert_eq!(config::load(&path).unwrap()["network"], newer);
    }

    #[test]
    fn optional_description_round_trips_without_weakening_command_validation() {
        let data = config::parse("network:\n  - command: ip addr\n").unwrap();
        assert_eq!(data["network"][0].description, "");
        assert!(config::parse("network:\n  - command: ''\n").is_err());
        assert!(config::parse("network:\n  - description: explanation\n").is_err());
    }
}
