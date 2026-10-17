//! These process-level checks run on both Windows and Linux release runners.
use std::path::Path;
use std::process::{Command, Output};

fn run(base: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
        .args(args)
        .env("XDG_CONFIG_HOME", base)
        .output()
        .unwrap()
}

#[test]
fn version_and_unicode_commands_work_across_platforms() {
    let dir = tempfile::tempdir().unwrap();
    let version = run(dir.path(), &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("cmdnavi {}", env!("CARGO_PKG_VERSION"))
    );
    for (command, description) in [("echo 日本語", "日本語の説明"), ("pwd", "")] {
        let output = run(dir.path(), &["add", "notes", command, description]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let shown = run(dir.path(), &["notes"]);
    assert!(shown.status.success());
    assert!(
        String::from_utf8(shown.stdout)
            .unwrap()
            .contains("日本語の説明")
    );
    let data = cmdnavi::config::load(&dir.path().join("cmdnavi/config.yaml")).unwrap();
    assert_eq!(data["notes"].len(), 2); // Verifies replacing an existing file, too.
    let search = run(dir.path(), &["--search", "日本語"]);
    assert!(search.status.success());
    assert!(
        String::from_utf8(search.stdout)
            .unwrap()
            .contains("echo 日本語")
    );
}

#[test]
fn native_config_location_works_without_xdg() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_cmdnavi"));
    command
        .args(["add", "notes", "echo test"])
        .env_remove("XDG_CONFIG_HOME");
    #[cfg(windows)]
    command.env("APPDATA", dir.path()).env_remove("HOME");
    #[cfg(not(windows))]
    command.env("HOME", dir.path());
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    #[cfg(windows)]
    let path = dir.path().join("cmdnavi/config.yaml");
    #[cfg(not(windows))]
    let path = dir.path().join(".config/cmdnavi/config.yaml");
    assert!(path.exists());
}

#[test]
fn japanese_categories_work_for_registration_completion_renaming_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    assert!(
        run(base, &["add", "障害調査", "ip addr", "調査用"])
            .status
            .success()
    );
    let output = run(
        base,
        &["--complete", "powershell", "1", "--", "edit", "障害"],
    );
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("障害調査")
    );
    assert!(
        run(base, &["--rename-category", "障害調査", "作業メモ"])
            .status
            .success()
    );
    assert_eq!(
        String::from_utf8(run(base, &["--print", "作業メモ", "1"]).stdout).unwrap(),
        "ip addr"
    );
    assert!(run(base, &["--undo"]).status.success());
    assert_eq!(
        String::from_utf8(run(base, &["--print", "障害調査", "1"]).stdout).unwrap(),
        "ip addr"
    );
}

#[test]
fn help_version_and_completion_scripts_work_without_a_config_location() {
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["add", "--help"],
        vec!["list", "-h"],
        vec!["--completions", "fish"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
            .args(&args)
            .env_remove("HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("APPDATA")
            .env_remove("USERPROFILE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
        .arg("--config-path")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("APPDATA")
        .env_remove("USERPROFILE")
        .output()
        .unwrap();
    assert!(!output.status.success());
}
