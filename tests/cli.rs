#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(config_home: &Path, args: &[&str], input: &str) -> Output {
    run_with_env(config_home, args, input, &[])
}

fn run_with_env(
    config_home: &Path,
    args: &[&str],
    input: &str,
    environment: &[(&str, &Path)],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cmdnavi"));
    command
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn script(directory: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    fs::write(&path, contents).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[test]
fn help_version_and_argument_errors_have_expected_exit_codes() {
    let dir = tempfile::tempdir().unwrap();

    let help = run(dir.path(), &["--help"], "");
    assert!(help.status.success());
    assert!(stdout(&help).contains("Usage:"));

    let version = run(dir.path(), &["--version"], "");
    assert!(version.status.success());
    assert_eq!(stdout(&version), "cmdnavi 1.0.0\n");

    for (args, expected) in [
        (vec!["add"], "needs a category"),
        (vec!["remove"], "needs a category"),
        (vec!["list", "extra"], "does not accept arguments"),
        (vec!["--unknown"], "unknown option"),
    ] {
        let output = run(dir.path(), &args, "");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains(expected));
    }
}

#[test]
fn data_persists_across_processes_and_registered_commands_are_not_executed() {
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("must-not-exist");
    let registered = format!("touch {}", sentinel.display());

    let added = run(
        dir.path(),
        &["add", "safety"],
        &format!("{registered}\n保存するだけ\n"),
    );
    assert!(added.status.success(), "{}", stderr(&added));
    assert!(!sentinel.exists());

    let shown = run(dir.path(), &["safety"], "");
    assert!(shown.status.success());
    assert!(stdout(&shown).contains(&registered));
    assert!(!sentinel.exists());

    let listed = run(dir.path(), &["list"], "");
    assert_eq!(stdout(&listed), "safety\n");

    let cancelled = run(dir.path(), &["remove", "safety"], "1\nN\n");
    assert!(cancelled.status.success());
    assert!(stdout(&cancelled).contains("Cancelled"));
    assert!(stdout(&run(dir.path(), &["safety"], "")).contains(&registered));

    let removed = run(dir.path(), &["remove", "safety"], "1\ny\n");
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(stdout(&removed).contains("Removed"));
    assert!(!sentinel.exists());
}

#[test]
fn invalid_editor_yaml_does_not_modify_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let added = run(dir.path(), &["add", "network"], "ip addr\nshow IP\n");
    assert!(added.status.success());
    let config_path = dir.path().join("cmdnavi/config.yaml");
    let before = fs::read(&config_path).unwrap();

    let editor = script(
        dir.path(),
        "invalid-editor.sh",
        "#!/bin/sh\nprintf '%s\\n' 'not: [valid' > \"$1\"\n",
    );

    let edited = run_with_env(dir.path(), &["edit", "network"], "", &[("VISUAL", &editor)]);
    assert!(!edited.status.success());
    assert!(stderr(&edited).contains("changes were not saved"));
    assert!(stderr(&edited).contains("Choose Y below to reopen"));
    assert!(stderr(&edited).contains("Your edits are still here:"));
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[test]
fn first_edit_and_missing_category_edit_use_templates() {
    let dir = tempfile::tempdir().unwrap();

    let full_template_editor = script(
        dir.path(),
        "check-full-template.sh",
        "#!/bin/sh\ngrep -q '^# network:' \"$1\"\n",
    );
    let first_edit = run_with_env(
        dir.path(),
        &["edit"],
        "",
        &[("VISUAL", &full_template_editor)],
    );
    assert!(first_edit.status.success(), "{}", stderr(&first_edit));
    assert!(!dir.path().join("cmdnavi/config.yaml").exists());

    let category_template_editor = script(
        dir.path(),
        "check-category-template.sh",
        "#!/bin/sh\ngrep -q '^# - command: ip addr' \"$1\"\n",
    );
    let unchanged = run_with_env(
        dir.path(),
        &["edit", "network"],
        "",
        &[("VISUAL", &category_template_editor)],
    );
    assert!(unchanged.status.success(), "{}", stderr(&unchanged));
    assert!(stdout(&run(dir.path(), &["list"], "")).is_empty());

    let creating_editor = script(
        dir.path(),
        "create-category.sh",
        "#!/bin/sh\nprintf '%s\\n' '- command: ip addr' '  description: show IP' > \"$1\"\n",
    );
    let created = run_with_env(
        dir.path(),
        &["edit", "network"],
        "",
        &[("VISUAL", &creating_editor)],
    );
    assert!(created.status.success(), "{}", stderr(&created));
    assert!(stdout(&run(dir.path(), &["network"], "")).contains("ip addr\n  show IP"));
}

#[test]
fn category_and_full_edits_work_through_an_external_editor() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(dir.path(), &["add", "network"], "old\nold\n")
            .status
            .success()
    );
    assert!(
        run(dir.path(), &["add", "docker"], "docker ps\ncontainers\n")
            .status
            .success()
    );

    let category_editor = script(
        dir.path(),
        "category-editor.sh",
        "#!/bin/sh\nprintf '%s\\n' '- command: ip addr' '  description: show IP' > \"$1\"\n",
    );
    let category_edit = run_with_env(
        dir.path(),
        &["edit", "network"],
        "",
        &[("VISUAL", &category_editor)],
    );
    assert!(category_edit.status.success(), "{}", stderr(&category_edit));
    assert!(stdout(&run(dir.path(), &["network"], "")).contains("ip addr\n  show IP"));
    assert!(stdout(&run(dir.path(), &["docker"], "")).contains("docker ps"));

    let full_editor = script(
        dir.path(),
        "full-editor.sh",
        "#!/bin/sh\nprintf '%s\\n' 'git:' '  - command: git status' '    description: show status' > \"$1\"\n",
    );
    let full_edit = run_with_env(dir.path(), &["edit"], "", &[("VISUAL", &full_editor)]);
    assert!(full_edit.status.success(), "{}", stderr(&full_edit));
    assert_eq!(stdout(&run(dir.path(), &["list"], "")), "git\n");
}

#[test]
fn an_edit_conflict_preserves_the_external_change() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(dir.path(), &["add", "network"], "original\noriginal\n")
            .status
            .success()
    );
    let editor = script(
        dir.path(),
        "conflicting-editor.sh",
        "#!/bin/sh\nprintf '%s\\n' '- command: editor' '  description: editor' > \"$1\"\nprintf '%s\\n' 'network:' '  - command: concurrent' '    description: concurrent' > \"$XDG_CONFIG_HOME/cmdnavi/config.yaml\"\n",
    );

    let edited = run_with_env(dir.path(), &["edit", "network"], "", &[("VISUAL", &editor)]);
    assert!(!edited.status.success());
    assert!(stderr(&edited).contains("configuration changed while the editor was open"));
    assert!(stderr(&edited).contains("Review the latest configuration"));
    assert!(stderr(&edited).contains("Your edited copy is still here:"));
    let shown = stdout(&run(dir.path(), &["network"], ""));
    assert!(shown.contains("concurrent"));
    assert!(!shown.contains("editor"));
}

#[test]
fn concurrent_adds_are_all_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let mut children = Vec::new();

    for index in 0..8 {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
            .args(["add", "parallel"])
            .env("XDG_CONFIG_HOME", dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        write!(
            child.stdin.take().unwrap(),
            "command-{index}\ndescription\n"
        )
        .unwrap();
        children.push(child);
    }

    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", stderr(&output));
    }

    let shown = run(dir.path(), &["parallel"], "");
    assert!(shown.status.success());
    let shown = stdout(&shown);
    for index in 0..8 {
        assert!(shown.contains(&format!("command-{index}")));
    }
}
