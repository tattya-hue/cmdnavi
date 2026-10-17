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
    assert_eq!(
        stdout(&version),
        concat!("cmdnavi ", env!("CARGO_PKG_VERSION"), "\n")
    );

    for (args, expected) in [
        (vec!["--print"], "needs a category"),
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

    let edited = run_with_env(
        dir.path(),
        &["--edit-yaml", "network"],
        "",
        &[("VISUAL", &editor)],
    );
    assert!(!edited.status.success());
    assert!(stderr(&edited).contains("changes were not saved"));
    assert!(stderr(&edited).contains("Choose Y below to reopen"));
    assert!(stderr(&edited).contains("Your edits are still here:"));
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[test]
fn ordinary_edit_selects_a_saved_item_without_opening_the_external_editor() {
    let dir = tempfile::tempdir().unwrap();
    for value in ["echo first", "echo second"] {
        assert!(
            run(dir.path(), &["add", "notes", value, "original"], "")
                .status
                .success()
        );
    }
    let sentinel = dir.path().join("editor-was-invoked");
    let editor = script(
        dir.path(),
        "unexpected-editor.sh",
        "#!/bin/sh\ntouch \"$XDG_CONFIG_HOME/editor-was-invoked\"\nexit 1\n",
    );
    let output = run_with_env(
        dir.path(),
        &["edit", "notes"],
        "2\necho updated\nupdated description\n",
        &[("VISUAL", &editor)],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run_with_env(
        dir.path(),
        &["edit"],
        "notes\n1\n\nnew description\n",
        &[("VISUAL", &editor)],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!sentinel.exists());
    let shown = stdout(&run(dir.path(), &["notes"], ""));
    assert!(shown.contains("echo first\n  new description"));
    assert!(shown.contains("echo updated\n  updated description"));
    let empty = tempfile::tempdir().unwrap();
    let output = run(empty.path(), &["edit"], "");
    assert!(output.status.success());
    assert!(stdout(&output).contains("No saved commands"));
    assert!(!empty.path().join("cmdnavi/config.yaml").exists());
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
        &["--edit-yaml"],
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
        &["--edit-yaml", "network"],
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
        &["--edit-yaml", "network"],
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
        &["--edit-yaml", "network"],
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
    let full_edit = run_with_env(
        dir.path(),
        &["--edit-yaml"],
        "",
        &[("VISUAL", &full_editor)],
    );
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

    let edited = run_with_env(
        dir.path(),
        &["--edit-yaml", "network"],
        "",
        &[("VISUAL", &editor)],
    );
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

#[test]
fn home_guides_first_use_and_summarizes_saved_commands() {
    let dir = tempfile::tempdir().unwrap();
    let empty = run(dir.path(), &[], "");
    assert!(empty.status.success());
    assert!(stdout(&empty).contains("cmdnavi add network"));
    assert!(!dir.path().join("cmdnavi/config.yaml").exists());
    assert!(
        run(dir.path(), &["add", "network"], "ip addr\nIPアドレス\n")
            .status
            .success()
    );
    let home = stdout(&run(dir.path(), &[], ""));
    assert!(home.contains("1 categories · 1 commands"));
    assert!(home.contains("network"));
    assert!(home.contains("--search"));
}

#[test]
fn search_finds_categories_commands_and_unicode_descriptions_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(
            dir.path(),
            &["add", "network"],
            "ip addr\nIPアドレスを確認\n"
        )
        .status
        .success()
    );
    assert!(
        run(
            dir.path(),
            &["add", "containers"],
            "docker ps\nコンテナ一覧\n"
        )
        .status
        .success()
    );
    let path = dir.path().join("cmdnavi/config.yaml");
    let before = fs::read(&path).unwrap();
    for query in ["NETWORK", "IP ADDR", "アドレス"] {
        let found = run(dir.path(), &["--search", query], "");
        assert!(found.status.success());
        assert!(stdout(&found).contains("[network]"));
        assert!(stdout(&found).contains("ip addr"));
        assert!(!stdout(&found).contains("docker ps"));
    }
    assert!(stdout(&run(dir.path(), &["--search", "missing"], "")).contains("No matches"));
    for args in [vec!["--search"], vec!["--search", "  "]] {
        assert!(!run(dir.path(), &args, "").status.success());
    }
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn removal_can_be_cancelled_before_selecting_and_accepts_yes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(dir.path(), &["add", "network"], "ip addr\nshow IP\n")
            .status
            .success()
    );
    for input in ["\n", "q\n", " Q \n"] {
        let cancelled = run(dir.path(), &["remove", "network"], input);
        assert!(cancelled.status.success());
        assert!(stdout(&cancelled).contains("Cancelled."));
        assert!(stdout(&run(dir.path(), &["network"], "")).contains("ip addr"));
    }
    assert!(
        run(dir.path(), &["remove", "network"], "1\n YES \n")
            .status
            .success()
    );
    assert!(stdout(&run(dir.path(), &["network"], "")).contains("cmdnavi add network"));
}

#[test]
fn multiline_descriptions_are_indented_and_command_help_never_mutates_data() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("cmdnavi")).unwrap();
    let path = dir.path().join("cmdnavi/config.yaml");
    fs::write(
        &path,
        "network:\n  - command: ip addr\n    description: |\n      first line\n      second line\n",
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(stdout(&run(dir.path(), &["network"], "")).contains("  first line\n  second line\n"));
    for command in ["add", "remove", "edit", "list"] {
        let help = run(dir.path(), &[command, "--help"], "");
        assert!(help.status.success());
        assert!(stdout(&help).contains("Usage:"));
    }
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn guided_add_selects_categories_retries_and_allows_optional_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let first = run(dir.path(), &["add"], "bad name\nnetwork\n\nip addr\n\n");
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("Enter a command to save"));
    let second = run(dir.path(), &["add"], "#99\n#1\nss -tulpn\nports\n");
    assert!(second.status.success(), "{}", stderr(&second));
    let saved = cmdnavi::config::load(&dir.path().join("cmdnavi/config.yaml")).unwrap();
    assert_eq!(saved["network"].len(), 2);
    assert_eq!(saved["network"][0].description, "");
    let default = run(dir.path(), &["add"], "\npwd\n\n");
    assert!(default.status.success(), "{}", stderr(&default));
    assert!(stdout(&run(dir.path(), &["general"], "")).contains("pwd"));
}

#[test]
fn add_cancellation_and_incomplete_input_never_save_partial_entries() {
    for input in [
        "/cancel\n",
        "network\n/cancel\n",
        "network\nip addr\n/cancel\n",
        "network\nip addr\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = run(dir.path(), &["add"], input);
        assert_eq!(output.status.success(), input.contains("/cancel"));
        assert!(!dir.path().join("cmdnavi/config.yaml").exists());
    }
}

#[test]
fn quick_add_deduplicates_and_updates_only_with_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cmdnavi/config.yaml");
    let command = "printf '%s' \"$(touch never-run)\"";
    assert!(
        run(dir.path(), &["add", "shell", command, "old"], "")
            .status
            .success()
    );
    let before = fs::read(&path).unwrap();
    assert!(
        run(dir.path(), &["add", "shell", command, "old"], "")
            .status
            .success()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        run(dir.path(), &["add", "shell", command, "new"], "n\n")
            .status
            .success()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        run(dir.path(), &["add", "shell", command, "new"], "yes\n")
            .status
            .success()
    );
    let saved = cmdnavi::config::load(&path).unwrap();
    assert_eq!(saved["shell"].len(), 1);
    assert_eq!(saved["shell"][0].command, command);
    assert_eq!(saved["shell"][0].description, "new");
    assert!(
        run(dir.path(), &["add", "shell", "pwd"], "")
            .status
            .success()
    );
}

#[test]
fn item_edit_keeps_values_clears_description_and_rejects_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cmdnavi/config.yaml");
    assert!(
        run(dir.path(), &["add", "network", "ip addr", "old"], "")
            .status
            .success()
    );
    assert!(
        run(dir.path(), &["edit", "network", "1"], "\nnew\n")
            .status
            .success()
    );
    let saved = cmdnavi::config::load(&path).unwrap();
    assert_eq!(saved["network"][0].command, "ip addr");
    assert_eq!(saved["network"][0].description, "new");
    assert!(
        run(dir.path(), &["edit", "network", "1"], "ip link\n/clear\n")
            .status
            .success()
    );
    assert_eq!(
        cmdnavi::config::load(&path).unwrap()["network"][0].description,
        ""
    );
    assert!(
        run(dir.path(), &["add", "network", "pwd"], "")
            .status
            .success()
    );
    let before = fs::read(&path).unwrap();
    assert!(
        !run(dir.path(), &["edit", "network", "1"], "pwd\n\n")
            .status
            .success()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    for number in ["0", "99", "bad"] {
        assert!(
            !run(dir.path(), &["edit", "network", number], "")
                .status
                .success()
        );
    }
    assert!(
        run(dir.path(), &["edit", "network", "1"], "/cancel\n")
            .status
            .success()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn remove_retries_invalid_selection_and_browser_requires_terminal() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(dir.path(), &["add", "network", "ip addr"], "")
            .status
            .success()
    );
    let removed = run(dir.path(), &["remove", "network"], "bad\n0\n99\n1\ny\n");
    assert!(removed.status.success(), "{}", stderr(&removed));
    let browse = run(dir.path(), &["--browse"], "");
    assert!(!browse.status.success());
    assert!(stderr(&browse).contains("needs a terminal"));
    assert!(!stdout(&browse).contains('\x1b'));
}
