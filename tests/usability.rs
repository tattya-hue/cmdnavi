use cmdnavi::{
    cli,
    completion::{self, Context},
    config,
    model::{CommandEntry, Config},
};
use std::io::Cursor;
use std::path::Path;
fn data() -> Config {
    Config::from([
        (
            "network".into(),
            vec![CommandEntry {
                command: "ip addr".into(),
                description: "アドレスを確認".into(),
            }],
        ),
        (
            "git".into(),
            vec![CommandEntry {
                command: "git status".into(),
                description: "状態".into(),
            }],
        ),
    ])
}
fn run(path: &Path, args: &[&str], input: &str) -> cmdnavi::Result<String> {
    let mut out = Vec::new();
    cli::run(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        path,
        &mut Cursor::new(input),
        &mut out,
    )?;
    Ok(String::from_utf8(out).unwrap())
}
fn values(words: &[&str], position: usize) -> Vec<String> {
    let words = words.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    completion::candidates(
        &data(),
        &completion::context(&words, position),
        words.get(position).map(String::as_str).unwrap_or(""),
    )
    .into_iter()
    .map(|c| c.value)
    .collect()
}
#[test]
fn completions_follow_argument_context_and_never_leak_into_descriptions_or_search() {
    let root = values(&[""], 0);
    assert!(root.contains(&"add".into()) && root.contains(&"network".into()));
    assert!(!root.contains(&"ip addr".into()));
    for action in [
        "add",
        "edit",
        "remove",
        "--show",
        "--print",
        "--copy",
        "--move",
        "--rename-category",
        "--remove-category",
        "--",
    ] {
        let categories = values(&[action, ""], 1);
        assert!(categories.contains(&"network".into()), "{action}");
        assert!(!categories.contains(&"ip addr".into()), "{action}");
        assert!(!categories.contains(&"add".into()), "{action}");
    }
    for action in ["edit", "remove", "--show", "--print", "--copy", "--move"] {
        let items = values(&[action, "network", ""], 2);
        assert!(items.contains(&"ip addr".into()), "{action}");
        assert!(items.contains(&"1".into()), "{action}");
        assert!(!items.contains(&"git status".into()), "{action}");
    }
    assert_eq!(values(&["add", "network", "ip"], 2), ["ip addr"]);
    assert!(!values(&["add", "network", ""], 2).contains(&"1".into()));
    for (words, position) in [
        (vec!["add", "network", "ip addr", ""], 3),
        (vec!["--search", ""], 1),
        (vec!["list", ""], 1),
        (vec!["--undo", ""], 1),
        (vec!["edit", "network", "ip addr", ""], 3),
    ] {
        assert!(values(&words, position).is_empty(), "{words:?}");
    }
    assert_eq!(values(&["--move", "network", "ip addr", "g"], 3), ["git"]);
    assert_eq!(values(&["--completions", "p"], 1), ["powershell"]);
    assert!(completion::candidates(&data(), &Context::None, "").is_empty());
}
#[test]
fn completion_reads_newly_saved_categories_without_writing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let args = ["--complete", "bash", "1", "--", "edit", ""];
    assert_eq!(run(&path, &args, "").unwrap(), "--help\0-h\0");
    assert!(!path.exists());
    config::save(&path, &data()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(run(&path, &args, "").unwrap().contains("network"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut updated = data();
    updated.insert("docker".into(), Vec::new());
    config::save(&path, &updated).unwrap();
    assert!(run(&path, &args, "").unwrap().contains("docker"));
    assert!(!path.with_extension("yaml.undo").exists());
}

#[test]
fn ambiguous_commands_complete_to_selectors_that_identify_the_right_item() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let commands = ["echo", "99", "echo", "#1", "2"];
    let data = Config::from([(
        "notes".into(),
        commands
            .iter()
            .map(|command| CommandEntry {
                command: (*command).into(),
                description: String::new(),
            })
            .collect(),
    )]);
    config::save(&path, &data).unwrap();
    for (prefix, expected) in [
        ("99", "#2"),
        ("echo", "#1"),
        ("echo", "#3"),
        ("#1", "#4"),
        ("2", "#5"),
    ] {
        let candidates = completion::candidates(&data, &Context::Items("notes".into()), prefix);
        assert!(
            candidates.iter().any(|c| c.value == expected),
            "{prefix}: {candidates:?}"
        );
        let index = cmdnavi::interaction::resolve_item(&data["notes"], expected).unwrap();
        assert_eq!(
            run(&path, &["--print", "notes", expected], "").unwrap(),
            commands[index]
        );
    }
    assert_eq!(
        completion::candidates(&data, &Context::Commands("notes".into()), "99")[0].value,
        "99"
    );
}

#[test]
fn fish_tabbed_items_complete_by_index_without_truncating_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    run(&path, &["add", "notes", "printf\tvalue", "tabbed"], "").unwrap();
    let output = run(
        &path,
        &[
            "--complete",
            "fish",
            "2",
            "--",
            "--print",
            "notes",
            "printf",
        ],
        "",
    )
    .unwrap();
    let records: Vec<_> = output.split('\0').filter(|r| !r.is_empty()).collect();
    assert_eq!(records.len(), 1);
    let (selector, description) = records[0].split_once('\t').unwrap();
    assert_eq!(selector, "#1");
    assert!(!description.contains('\t'));
    assert_eq!(
        run(&path, &["--print", "notes", selector], "").unwrap(),
        "printf\tvalue"
    );
    assert!(
        run(
            &path,
            &["--complete", "fish", "2", "--", "add", "notes", "printf"],
            ""
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn omitted_duplicate_description_is_noninteractive_and_preserves_saved_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    config::save(&path, &data()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        run(&path, &["add", "network", "ip addr"], "")
            .unwrap()
            .contains("Already saved")
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn form_navigation_words_stay_literal_in_arguments_and_piped_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    run(&path, &["add", "notes"], "echo script\n/back\n").unwrap();
    run(&path, &["add", "notes", "echo args", "/category"], "").unwrap();
    let data = config::load(&path).unwrap();
    assert_eq!(data["notes"][0].description, "/back");
    assert_eq!(data["notes"][1].description, "/category");
}
#[test]
fn unsafe_input_and_extra_lines_are_rejected_before_saving() {
    for input in [
        "ip\x1b[DZ\ndescription\n",
        "echo first\necho second\nreal description\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        assert!(run(&path, &["add", "network"], input).is_err());
        assert!(!path.exists());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    run(
        &path,
        &["add", "network", "echo first\necho second", "multiline"],
        "",
    )
    .unwrap();
    assert_eq!(
        run(&path, &["--print", "network", "1"], "").unwrap(),
        "echo first\necho second"
    );
}
#[test]
fn shared_search_numbered_display_and_leading_hyphen_categories_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    config::save(&path, &data()).unwrap();
    assert!(
        run(&path, &["--search", "NETWORK addr アドレス"], "")
            .unwrap()
            .contains("ip addr")
    );
    assert!(run(&path, &["network"], "").unwrap().contains("[1]"));
    run(&path, &["add", "-ops", "pwd"], "").unwrap();
    assert!(run(&path, &["--show", "-ops"], "").unwrap().contains("pwd"));
    assert!(run(&path, &["--", "-ops"], "").unwrap().contains("pwd"));
}
#[test]
fn move_rename_remove_and_undo_preserve_data_and_check_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    config::save(&path, &data()).unwrap();
    run(&path, &["--move", "network", "ip addr", "tools"], "").unwrap();
    assert_eq!(
        run(&path, &["--print", "tools", "ip addr"], "").unwrap(),
        "ip addr"
    );
    run(&path, &["--rename-category", "tools", "ops"], "").unwrap();
    run(&path, &["--remove-category", "ops"], "yes\n").unwrap();
    assert!(!config::load(&path).unwrap().contains_key("ops"));
    run(&path, &["--undo"], "").unwrap();
    assert!(config::load(&path).unwrap().contains_key("ops"));
    run(&path, &["--undo"], "").unwrap();
    assert!(!config::load(&path).unwrap().contains_key("ops"));
    let mut external = config::load(&path).unwrap();
    external.insert("external".into(), vec![]);
    config::save(&path, &external).unwrap();
    assert!(run(&path, &["--undo"], "").is_err());
    assert_eq!(config::load(&path).unwrap(), external);
}
#[test]
fn item_actions_accept_exact_command_and_shell_metacharacters_remain_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let value = "echo '日本語'; $(touch never-execute)\nsecond line";
    run(&path, &["add", "shell", value, "description"], "").unwrap();
    let json = run(
        &path,
        &[
            "--complete",
            "powershell",
            "2",
            "--",
            "--print",
            "shell",
            "__CMDNAVI_PREFIX__echo",
        ],
        "",
    )
    .unwrap();
    let values: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(values[0]["value"], value);
    assert_eq!(
        values[0]["insert"],
        completion::shell_quote(value, "powershell")
    );
    let bash = run(
        &path,
        &["--complete", "bash", "2", "--", "--print", "shell", "echo"],
        "",
    )
    .unwrap();
    assert_eq!(bash.lines().count(), 1);
    assert!(bash.contains("\\n"));
    assert_eq!(run(&path, &["--print", "shell", value], "").unwrap(), value);
    run(&path, &["edit", "shell", value], "\nnew description\n").unwrap();
    assert_eq!(
        config::load(&path).unwrap()["shell"][0].description,
        "new description"
    );
}

#[test]
fn bash_partial_quotes_decode_as_text_without_evaluating_expansions() {
    for (word, expected) in [
        ("ip\\ addr", "ip addr"),
        ("'ip addr", "ip addr"),
        ("\"ip addr", "ip addr"),
        ("$'ip add", "ip add"),
        ("$'a\\n日本語\\'x'", "a\n日本語'x"),
        ("\\$(touch\\ never)", "$(touch never)"),
    ] {
        assert_eq!(completion::bash_word(word), expected);
    }
    let value = "echo '日本語'; $(touch never-execute)\nnext";
    assert_eq!(
        completion::bash_word(&completion::shell_quote(value, "bash")),
        value
    );
}

#[cfg(unix)]
#[test]
fn wsl_clipboard_fallback_sends_utf8_without_interpreting_commands() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let command = "echo '日本語'; $(touch never-execute)\nnext";
    run(&path, &["add", "shell", command], "").unwrap();
    let helper = dir.path().join("powershell.exe");
    std::fs::write(&helper, "#!/bin/sh\n/bin/cat > \"$CMDNAVI_CAPTURE\"\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    // The executable resolves XDG_CONFIG_HOME/cmdnavi/config.yaml.
    let config_dir = dir.path().join("cmdnavi");
    std::fs::create_dir(&config_dir).unwrap();
    std::fs::copy(&path, config_dir.join("config.yaml")).unwrap();
    let captured = dir.path().join("captured");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
        .args(["--copy", "shell", "1"])
        .env("XDG_CONFIG_HOME", dir.path())
        .env("PATH", dir.path())
        .env("WSL_INTEROP", "test")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env("CMDNAVI_CAPTURE", &captured)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(captured).unwrap(), command);
}

#[cfg(unix)]
#[test]
fn stalled_clipboard_helper_times_out_instead_of_hanging() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("cmdnavi");
    std::fs::create_dir(&config_dir).unwrap();
    config::save(&config_dir.join("config.yaml"), &data()).unwrap();
    let helper = dir.path().join("wl-copy");
    std::fs::write(&helper, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let started = std::time::Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cmdnavi"))
        .args(["--copy", "network", "1"])
        .env("XDG_CONFIG_HOME", dir.path())
        .env("PATH", dir.path())
        .env("WAYLAND_DISPLAY", "test")
        .env_remove("DISPLAY")
        .env_remove("WSL_INTEROP")
        .env_remove("WSL_DISTRO_NAME")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("timed out"));
    assert!(started.elapsed() < std::time::Duration::from_secs(6));
}
