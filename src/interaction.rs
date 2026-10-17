//! Line-based forms shared by the CLI and terminal browser.
use crate::cli::read_line;
use crate::model::{CommandEntry, validate_category};
use crate::{Error, Result, config};
use std::io::{BufRead, Write};
use std::path::Path;

fn prompt(input: &mut dyn BufRead, output: &mut dyn Write, label: &str) -> Result<Option<String>> {
    write!(output, "{label}")?;
    output.flush()?;
    let value = read_line(input)?;
    if value.trim() == "/cancel" {
        writeln!(output, "Cancelled.")?;
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub fn choose_category(
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Option<String>> {
    let data = config::load(path)?;
    let categories: Vec<_> = data.keys().collect();
    writeln!(
        output,
        "Choose a category or type a new name. /cancel to cancel."
    )?;
    for (index, category) in categories.iter().enumerate() {
        writeln!(
            output,
            "  #{}  {} ({} commands)",
            index + 1,
            category,
            data[*category].len()
        )?;
    }
    loop {
        let Some(value) = prompt(
            input,
            output,
            "Category (name / #number, Enter = general): ",
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
                .and_then(|n| categories.get(n))
            {
                Some(category) => category.as_str(),
                None => {
                    writeln!(output, "Choose one of the #numbers shown above.")?;
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
            Err(message) => writeln!(output, "{message}")?,
        }
    }
}

pub fn add(
    path: &Path,
    category: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    validate_category(category).map_err(Error)?;
    // Detect an unreadable configuration before asking the user to type anything.
    config::load(path)?;
    writeln!(output, "Add to [{category}] · /cancel to cancel")?;
    let command = loop {
        let Some(command) = prompt(input, output, "Command: ")? else {
            return Ok(());
        };
        if !command.trim().is_empty() {
            break command;
        }
        writeln!(output, "Enter a command to save.")?;
    };
    let Some(description) = prompt(input, output, "Description (optional, Enter to skip): ")?
    else {
        return Ok(());
    };
    save_entry(
        path,
        category,
        CommandEntry {
            command,
            description,
        },
        input,
        output,
    )
}

pub fn save_entry(
    path: &Path,
    category: &str,
    entry: CommandEntry,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    validate_category(category).map_err(Error)?;
    entry.validate().map_err(|message| Error(message.into()))?;
    let snapshot = config::load(path)?;
    let original = snapshot.get(category);
    let duplicate = original.and_then(|entries| {
        entries
            .iter()
            .position(|saved| saved.command.trim() == entry.command.trim())
    });
    if let Some(index) = duplicate {
        let saved = &original.unwrap()[index];
        if saved.description == entry.description {
            writeln!(output, "Already saved in [{category}]. No changes.")?;
            return Ok(());
        }
        writeln!(
            output,
            "Already saved in [{category}]:\n  {}\n  {}",
            saved.command, saved.description
        )?;
        let Some(answer) = prompt(input, output, "Replace its description? [y/N]: ")? else {
            return Ok(());
        };
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            writeln!(output, "Cancelled.")?;
            return Ok(());
        }
    }
    config::update(path, |data| {
        if let Some(index) = duplicate {
            if data.get(category) != original {
                return Err(conflict());
            }
            data.get_mut(category).unwrap()[index].description = entry.description;
        } else {
            let entries = data.entry(category.into()).or_default();
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
        writeln!(output, "Created category \"{category}\".")?;
    }
    writeln!(
        output,
        "{} command in [{category}].",
        if duplicate.is_some() {
            "Updated"
        } else {
            "Saved"
        }
    )?;
    Ok(())
}

fn conflict() -> Error {
    Error(
        "This category changed while you were editing. Nothing was overwritten. Please try again."
            .into(),
    )
}

/// `index` is zero-based; the snapshot prevents editing a different item after a concurrent change.
pub fn edit_entry(
    path: &Path,
    category: &str,
    index: usize,
    snapshot: &[CommandEntry],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    let old = snapshot
        .get(index)
        .ok_or_else(|| Error("Choose an existing item number.".into()))?;
    writeln!(
        output,
        "Edit [{category}] · Enter keeps the current value · /cancel cancels\nCommand: {}\nDescription: {}",
        old.command, old.description
    )?;
    let command = loop {
        let Some(value) = prompt(input, output, "New command: ")? else {
            return Ok(());
        };
        if value.is_empty() {
            break old.command.clone();
        }
        if !value.trim().is_empty() {
            break value;
        }
        writeln!(
            output,
            "Enter a command, or press Enter to keep the current one."
        )?;
    };
    let Some(value) = prompt(input, output, "New description (/clear to remove): ")? else {
        return Ok(());
    };
    let description = match value.as_str() {
        "" => old.description.clone(),
        "/clear" => String::new(),
        _ => value,
    };
    let edited = CommandEntry {
        command,
        description,
    };
    if edited == *old {
        writeln!(output, "No changes.")?;
        return Ok(());
    }
    config::update(path, |data| {
        if data.get(category).map(Vec::as_slice) != Some(snapshot) {
            return Err(conflict());
        }
        let entries = data.get_mut(category).unwrap();
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
    writeln!(output, "Updated command in [{category}].")?;
    Ok(())
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
