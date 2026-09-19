use crate::model::{Config, validate_config};
use crate::{Error, Result};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn config_path() -> Result<PathBuf> {
    if let Some(base) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(base).join("cmdnavi/config.yaml"));
    }
    let home = env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            Error(
                "the configuration location could not be determined.\nSet HOME or XDG_CONFIG_HOME and try again."
                    .into(),
            )
        })?;
    Ok(PathBuf::from(home).join(".config/cmdnavi/config.yaml"))
}

pub fn load(path: &Path) -> Result<Config> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::new()),
        Err(error) => {
            return Err(Error(format!(
                "the configuration could not be read.\nCheck that this file is readable and has the correct permissions:\n  {}\n\nSystem message: {error}",
                path.display()
            )));
        }
    };
    parse(&contents).map_err(|error| {
        Error(format!(
            "the configuration contains invalid YAML, so it could not be loaded.\nBack up the file, open it in a text editor, correct the YAML, and try again:\n  {}\n\nYAML message: {error}",
            path.display(),
        ))
    })
}

pub fn parse(contents: &str) -> std::result::Result<Config, String> {
    if is_effectively_empty(contents) {
        return Ok(Config::new());
    }
    let config: Config = yaml_serde::from_str(contents).map_err(|e| e.to_string())?;
    validate_config(&config)?;
    Ok(config)
}

pub fn is_effectively_empty(contents: &str) -> bool {
    contents
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
}

pub fn save(path: &Path, config: &Config) -> Result<()> {
    validate_config(config).map_err(Error)?;
    let yaml = yaml_serde::to_string(config)
        .map_err(|e| {
            Error(format!(
                "the configuration could not be prepared for saving.\nYour existing file was left unchanged. Please report this problem.\n\nTechnical message: {e}"
            ))
        })?;
    save_text_atomically(path, &yaml)
}

/// Runs a read-modify-write operation while holding the configuration lock.
///
/// The lock is kept outside `config.yaml`, because that file is replaced with
/// `rename` when it is saved. Locking the replaced file itself would not
/// coordinate processes that opened it before and after the rename.
pub fn update<T>(path: &Path, operation: impl FnOnce(&mut Config) -> Result<T>) -> Result<T> {
    let parent = path
        .parent()
        .ok_or_else(|| {
            Error(
                "the configuration path cannot be used.\nSet XDG_CONFIG_HOME to a valid directory and try again."
                    .into(),
            )
        })?;
    fs::create_dir_all(parent).map_err(|e| {
        Error(format!(
            "the configuration directory could not be created.\nCheck its parent permissions, or set XDG_CONFIG_HOME to a writable directory:\n  {}\n\nSystem message: {e}",
            parent.display(),
        ))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error(
                "the configuration file name cannot be used.\nSet XDG_CONFIG_HOME to a valid directory and try again."
                    .into(),
            )
        })?;
    let lock_path = parent.join(format!("{file_name}.lock"));
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| {
            Error(format!(
                "the configuration could not be locked for writing.\nCheck that the configuration directory is writable, then try again.\n\nLock file: {}\nSystem message: {e}",
                lock_path.display(),
            ))
        })?;
    lock_file.lock().map_err(|e| {
        Error(format!(
            "the configuration could not be locked for writing.\nCheck the lock file permissions and try again.\n\nSystem message: {e}"
        ))
    })?;

    let mut config = load(path)?;
    let result = operation(&mut config)?;
    save(path, &config)?;
    Ok(result)
}

fn save_text_atomically(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| {
            Error(
                "the configuration path cannot be used.\nSet XDG_CONFIG_HOME to a valid directory and try again."
                    .into(),
            )
        })?;
    fs::create_dir_all(parent).map_err(|e| {
        Error(format!(
            "the configuration directory could not be created.\nCheck its parent permissions, or set XDG_CONFIG_HOME to a writable directory:\n  {}\n\nSystem message: {e}",
            parent.display(),
        ))
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".config.yaml.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| {
                Error(format!(
                    "the configuration could not be saved.\nCheck free disk space and the configuration directory permissions, then try again.\n\nSystem message: {e}"
                ))
            })?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path).map_err(|e| {
            Error(format!(
                "the configuration could not be saved.\nCheck free disk space and the permissions for this file, then try again:\n  {}\n\nSystem message: {e}",
                path.display(),
            ))
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CommandEntry;

    #[test]
    fn yaml_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.yaml");
        let mut config = Config::new();
        config.insert(
            "network".into(),
            vec![CommandEntry {
                command: "ip addr".into(),
                description: "show addresses".into(),
            }],
        );
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
    }

    #[test]
    fn concurrent_updates_do_not_overwrite_each_other() {
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("config.yaml"));
        let workers = 8;
        let barrier = Arc::new(Barrier::new(workers));
        let handles: Vec<_> = (0..workers)
            .map(|index| {
                let path = Arc::clone(&path);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    update(&path, |config| {
                        config
                            .entry("network".into())
                            .or_default()
                            .push(CommandEntry {
                                command: format!("command-{index}"),
                                description: "description".into(),
                            });
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(load(&path).unwrap()["network"].len(), workers);
    }
}
