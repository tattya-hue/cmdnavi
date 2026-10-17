use crate::{Error, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Send text through stdin, never through a shell or command-line interpolation.
#[cfg(not(windows))]
pub fn copy(text: &str) -> Result<()> {
    let mut candidates: Vec<(&str, &[&str])> = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        candidates.push(("wl-copy", &[]));
    }
    if std::env::var_os("DISPLAY").is_some() {
        candidates.push(("xclip", &["-selection", "clipboard"]));
        candidates.push(("xsel", &["--clipboard", "--input"]));
    }
    for (program, args) in candidates {
        if send(program, args, text).is_ok() {
            return Ok(());
        }
    }
    Err(Error("Could not copy. Install wl-clipboard (Wayland) or xclip/xsel (X11), and use a desktop session. Press Esc to exit; `cmdnavi <category>` displays the full text for manual copying.".into()))
}

#[cfg(windows)]
pub fn copy(text: &str) -> Result<()> {
    // The script is constant; command text is data read from UTF-8 stdin.
    send("powershell.exe", &[
        "-NoProfile", "-NonInteractive", "-STA", "-Command",
        "$ErrorActionPreference = 'Stop'; $reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), [System.Text.Encoding]::UTF8); Set-Clipboard -Value $reader.ReadToEnd()"
    ], text).map_err(|_| Error("Could not copy. Check that Windows PowerShell and the clipboard are available. Use `cmdnavi <category>` for manual copying.".into()))
}

fn send(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let written = child.stdin.take().unwrap().write_all(text.as_bytes());
    let status = child.wait()?;
    written?;
    if !status.success() {
        return Err(Error("Clipboard helper failed.".into()));
    }
    Ok(())
}
