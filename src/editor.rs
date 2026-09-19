use crate::config;
use crate::model::{CommandEntry, Config};
use crate::{Error, Result};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const FULL_EDIT_TEMPLATE: &str = "# cmdnavi configuration\n\
# Add a category and command by removing the leading '#' characters.\n\
# Both command and description are required.\n\
# network:\n\
#   - command: ip addr\n\
#     description: IPアドレスを確認する\n";

const CATEGORY_EDIT_TEMPLATE: &str = "# Add a command by removing the leading '#' characters.\n\
# Both command and description are required.\n\
# - command: ip addr\n\
#   description: IPアドレスを確認する\n";

pub trait Editor {
    fn edit(&self, path: &Path) -> Result<()>;
}

pub struct ExternalEditor;

impl Editor for ExternalEditor {
    fn edit(&self, path: &Path) -> Result<()> {
        let specification = env::var("VISUAL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| "vi".into());
        let mut parts = shell_words::split(&specification)
            .map_err(|e| {
                Error(format!(
                    "the editor setting is invalid.\nCorrect $VISUAL or $EDITOR, or unset it to use vi.\n\nTechnical message: {e}"
                ))
            })?;
        if parts.is_empty() {
            return Err(Error(
                "no editor command was provided.\nUnset $VISUAL or $EDITOR to use vi.".into(),
            ));
        }
        let program = parts.remove(0);
        let status = Command::new(&program)
            .args(parts)
            .arg(path)
            .status()
            .map_err(|e| {
                Error(format!(
                    "the editor \"{program}\" could not be started.\nInstall it, or set $VISUAL or $EDITOR to an available command.\n\nSystem message: {e}"
                ))
            })?;
        if !status.success() {
            return Err(Error(format!(
                "the editor closed with an error.\nCheck the command in $VISUAL or $EDITOR and try again.\n\nExit status: {status}"
            )));
        }
        Ok(())
    }
}

pub fn edit_all(path: &Path, editor: &dyn Editor) -> Result<()> {
    edit_all_impl(path, editor, &mut |_, _| Ok(InvalidEditDecision::Abort))
}

pub fn edit_all_interactive(
    path: &Path,
    editor: &dyn Editor,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    edit_all_impl(path, editor, &mut |error, temporary| {
        prompt_to_retry(error, temporary, "cmdnavi edit", input, output)
    })
}

fn edit_all_impl(
    path: &Path,
    editor: &dyn Editor,
    retry: &mut dyn FnMut(&Error, &Path) -> Result<InvalidEditDecision>,
) -> Result<()> {
    let original = config::load(path)?;
    let yaml = if original.is_empty() {
        FULL_EDIT_TEMPLATE.to_owned()
    } else {
        yaml_serde::to_string(&original)
            .map_err(|e| {
                Error(format!(
                    "the configuration could not be prepared for editing.\nYour existing file was left unchanged. Please report this problem.\n\nTechnical message: {e}"
                ))
            })?
    };
    let temporary = create_edit_file(&yaml)?;
    let Some(edited) = edit_until_valid(&temporary, editor, retry, read_and_parse_config)? else {
        return Ok(());
    };
    if original.is_empty() && edited.is_empty() {
        fs::remove_file(&temporary)
            .map_err(|e| {
                Error(format!(
                    "the unchanged edit file could not be removed.\nYou can remove the temporary file manually if it still exists.\n\nSystem message: {e}"
                ))
            })?;
        return Ok(());
    }
    config::update(path, |current| {
        if current != &original {
            return Err(edit_conflict(&temporary));
        }
        *current = edited;
        Ok(())
    })?;
    remove_completed_edit_file(&temporary)?;
    Ok(())
}

pub fn edit_category(path: &Path, category: &str, editor: &dyn Editor) -> Result<()> {
    edit_category_impl(path, category, editor, &mut |_, _| {
        Ok(InvalidEditDecision::Abort)
    })
}

pub fn edit_category_interactive(
    path: &Path,
    category: &str,
    editor: &dyn Editor,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    let resume_command = format!("cmdnavi edit {category}");
    edit_category_impl(path, category, editor, &mut |error, temporary| {
        prompt_to_retry(error, temporary, &resume_command, input, output)
    })
}

fn edit_category_impl(
    path: &Path,
    category: &str,
    editor: &dyn Editor,
    retry: &mut dyn FnMut(&Error, &Path) -> Result<InvalidEditDecision>,
) -> Result<()> {
    let config_data = config::load(path)?;
    let original_entries = config_data.get(category).cloned();
    let yaml = match &original_entries {
        Some(entries) if !entries.is_empty() => yaml_serde::to_string(entries)
            .map_err(|e| {
                Error(format!(
                    "the category could not be prepared for editing.\nYour existing file was left unchanged. Please report this problem.\n\nTechnical message: {e}"
                ))
            })?,
        _ => CATEGORY_EDIT_TEMPLATE.to_owned(),
    };
    let temporary = create_edit_file(&yaml)?;
    let Some(edited) = edit_until_valid(&temporary, editor, retry, read_and_parse_entries)? else {
        return Ok(());
    };
    if original_entries.is_none() && edited.is_empty() {
        fs::remove_file(&temporary)
            .map_err(|e| {
                Error(format!(
                    "the unchanged edit file could not be removed.\nYou can remove the temporary file manually if it still exists.\n\nSystem message: {e}"
                ))
            })?;
        return Ok(());
    }
    config::update(path, |current| {
        if current.get(category) != original_entries.as_ref() {
            return Err(edit_conflict(&temporary));
        }
        current.insert(category.to_owned(), edited);
        Ok(())
    })?;
    remove_completed_edit_file(&temporary)?;
    Ok(())
}

fn create_edit_file(contents: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = env::temp_dir().join(format!("cmdnavi-edit-{}-{nonce}.yaml", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| {
            Error(format!(
                "the editor could not be opened because its temporary file could not be created.\nCheck that the temporary directory is writable and has free space.\n\nTemporary file: {}\nSystem message: {e}",
                path.display(),
            ))
        })?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

fn invalid_yaml(path: &Path, detail: impl std::fmt::Display) -> Error {
    Error(format!(
        "the changes were not saved because the edited YAML is invalid.\nChoose Y below to reopen the same file, then correct the YAML and save it.\n\nYour edits are still here:\n  {}\n\nYAML message: {detail}",
        path.display(),
    ))
}

fn edit_until_valid<T>(
    path: &Path,
    editor: &dyn Editor,
    retry: &mut dyn FnMut(&Error, &Path) -> Result<InvalidEditDecision>,
    parse: impl Fn(&Path) -> Result<T>,
) -> Result<Option<T>> {
    loop {
        if let Err(error) = editor.edit(path) {
            return Err(Error(format!(
                "{error}\n\nThe configuration was not changed.\nTemporary edit file: {}",
                path.display()
            )));
        }
        match parse(path) {
            Ok(value) => return Ok(Some(value)),
            Err(error) => match retry(&error, path)? {
                InvalidEditDecision::Retry => {}
                InvalidEditDecision::Cancel => return Ok(None),
                InvalidEditDecision::Abort => return Err(error),
            },
        }
    }
}

enum InvalidEditDecision {
    Retry,
    Cancel,
    Abort,
}

fn prompt_to_retry(
    error: &Error,
    path: &Path,
    resume_command: &str,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<InvalidEditDecision> {
    writeln!(output, "\n{error}")?;
    write!(output, "\nOpen the same file again? [Y/n]: ")?;
    output.flush()?;
    loop {
        let mut answer = String::new();
        if input.read_line(&mut answer)? == 0 {
            writeln!(
                output,
                "Editing stopped. To recover later, open {} and copy the corrected contents into `{resume_command}`.",
                path.display()
            )?;
            return Ok(InvalidEditDecision::Abort);
        }
        let answer = answer.trim();
        if answer.is_empty() || answer.eq_ignore_ascii_case("y") {
            return Ok(InvalidEditDecision::Retry);
        }
        if answer.eq_ignore_ascii_case("n") {
            writeln!(
                output,
                "Editing stopped. To recover later, open {} and copy the corrected contents into `{resume_command}`.",
                path.display()
            )?;
            return Ok(InvalidEditDecision::Cancel);
        }
        write!(output, "Please enter Y to reopen the file or N to stop: ")?;
        output.flush()?;
    }
}

fn remove_completed_edit_file(path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(|e| {
        Error(format!(
            "the configuration was saved, but its temporary edit file remains.\nYou can remove this file manually:\n  {}\n\nSystem message: {e}",
            path.display(),
        ))
    })
}

fn edit_conflict(path: &Path) -> Error {
    Error(format!(
        "the changes were not saved because the configuration changed while the editor was open.\nReview the latest configuration, run the edit command again, and merge your changes.\n\nYour edited copy is still here:\n  {}",
        path.display()
    ))
}

fn read_and_parse_config(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)?;
    config::parse(&contents).map_err(|e| invalid_yaml(path, e))
}

fn read_and_parse_entries(path: &Path) -> Result<Vec<CommandEntry>> {
    let contents = fs::read_to_string(path)?;
    if config::is_effectively_empty(&contents) {
        return Ok(Vec::new());
    }
    let entries: Vec<CommandEntry> =
        yaml_serde::from_str(&contents).map_err(|e| invalid_yaml(path, e))?;
    for entry in &entries {
        entry.validate().map_err(|e| invalid_yaml(path, e))?;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct ReplacingEditor(&'static str);
    impl Editor for ReplacingEditor {
        fn edit(&self, path: &Path) -> Result<()> {
            fs::write(path, self.0)?;
            Ok(())
        }
    }

    struct ExpectingTemplate(&'static str);

    impl Editor for ExpectingTemplate {
        fn edit(&self, path: &Path) -> Result<()> {
            let contents = fs::read_to_string(path)?;
            assert!(contents.contains(self.0));
            Ok(())
        }
    }

    struct InvalidThenValidEditor {
        calls: Cell<usize>,
    }

    impl Editor for InvalidThenValidEditor {
        fn edit(&self, path: &Path) -> Result<()> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            if call == 0 {
                fs::write(path, "- command: ip addr\n")?;
            } else {
                fs::write(path, "- command: ip addr\n  description: show IP\n")?;
            }
            Ok(())
        }
    }

    struct ConcurrentChangeEditor {
        config_path: PathBuf,
    }

    impl Editor for ConcurrentChangeEditor {
        fn edit(&self, path: &Path) -> Result<()> {
            fs::write(path, "- command: edited\n  description: edited\n")?;
            config::update(&self.config_path, |config| {
                config.insert("network".into(), vec![entry("concurrent")]);
                Ok(())
            })
        }
    }

    fn entry(command: &str) -> CommandEntry {
        CommandEntry {
            command: command.into(),
            description: "description".into(),
        }
    }

    #[test]
    fn category_edit_preserves_other_categories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut original = Config::new();
        original.insert("network".into(), vec![entry("old")]);
        original.insert("docker".into(), vec![entry("docker ps")]);
        config::save(&path, &original).unwrap();
        edit_category(
            &path,
            "network",
            &ReplacingEditor("- command: new\n  description: changed\n"),
        )
        .unwrap();
        let result = config::load(&path).unwrap();
        assert_eq!(result["network"][0].command, "new");
        assert_eq!(result["docker"], original["docker"]);
    }

    #[test]
    fn invalid_category_edit_does_not_modify_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut original = Config::new();
        original.insert("network".into(), vec![entry("old")]);
        config::save(&path, &original).unwrap();
        assert!(edit_category(&path, "network", &ReplacingEditor("not: [valid")).is_err());
        assert_eq!(config::load(&path).unwrap(), original);
    }

    #[test]
    fn invalid_full_edit_does_not_modify_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut original = Config::new();
        original.insert("network".into(), vec![entry("ip addr")]);
        config::save(&path, &original).unwrap();

        let error = edit_all(&path, &ReplacingEditor("network: [broken"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("changes were not saved"));
        assert!(error.contains("Choose Y below to reopen"));
        assert!(error.contains("Your edits are still here:"));
        assert_eq!(config::load(&path).unwrap(), original);
    }

    #[test]
    fn category_edit_detects_a_concurrent_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut original = Config::new();
        original.insert("network".into(), vec![entry("original")]);
        config::save(&path, &original).unwrap();

        let error = edit_category(
            &path,
            "network",
            &ConcurrentChangeEditor {
                config_path: path.clone(),
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("configuration changed while the editor was open"));
        assert!(error.contains("Review the latest configuration"));
        assert!(error.contains("Your edited copy is still here:"));
        assert_eq!(
            config::load(&path).unwrap()["network"][0].command,
            "concurrent"
        );
    }

    #[test]
    fn editing_a_missing_category_shows_a_template_and_creates_nothing_if_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        edit_category(&path, "network", &ExpectingTemplate("# - command: ip addr")).unwrap();

        assert!(!path.exists());
        assert!(!config::load(&path).unwrap().contains_key("network"));
    }

    #[test]
    fn editing_a_missing_category_creates_it_when_an_entry_is_added() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        edit_category(
            &path,
            "network",
            &ReplacingEditor("- command: ip addr\n  description: show IP\n"),
        )
        .unwrap();

        assert_eq!(
            config::load(&path).unwrap()["network"][0].command,
            "ip addr"
        );
    }

    #[test]
    fn a_command_without_a_description_does_not_create_a_category() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        let error = edit_category(&path, "network", &ReplacingEditor("- command: ip addr\n"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("edited YAML is invalid"));
        assert!(error.contains("Choose Y below to reopen"));
        assert!(error.contains("Your edits are still here:"));
        assert!(!config::load(&path).unwrap().contains_key("network"));
    }

    #[test]
    fn first_full_edit_shows_a_template_and_creates_nothing_if_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        edit_all(&path, &ExpectingTemplate("# network:")).unwrap();

        assert!(!path.exists());
        assert!(config::load(&path).unwrap().is_empty());
    }

    #[test]
    fn invalid_yaml_can_be_fixed_by_reopening_the_same_edit_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let editor = InvalidThenValidEditor {
            calls: Cell::new(0),
        };
        let mut input = std::io::Cursor::new("\n");
        let mut output = Vec::new();

        edit_category_interactive(&path, "network", &editor, &mut input, &mut output).unwrap();

        assert_eq!(editor.calls.get(), 2);
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Open the same file again?")
        );
        assert_eq!(
            config::load(&path).unwrap()["network"][0].command,
            "ip addr"
        );
    }

    #[test]
    fn declining_to_reopen_is_a_successful_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut input = std::io::Cursor::new("maybe\nn\n");
        let mut output = Vec::new();

        edit_category_interactive(
            &path,
            "network",
            &ReplacingEditor("- command: ip addr\n"),
            &mut input,
            &mut output,
        )
        .unwrap();

        assert!(!path.exists());
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Please enter Y"));
        assert!(output.contains("Editing stopped"));
    }
}
