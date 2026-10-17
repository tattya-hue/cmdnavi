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
    // WSL can use Windows PowerShell when no Linux clipboard helper is available.
    let wsl =
        std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some();
    let mut failures = Vec::new();
    for (program, args) in candidates {
        match send(program, args, text) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("{program}: {error}")),
        }
    }
    if wsl && windows_copy(text).is_ok() {
        return Ok(());
    }
    Err(Error(format!(
        "Could not copy. Use wl-clipboard (Wayland) or xclip/xsel (X11), or Windows PowerShell in WSL.\nFor manual copying: `cmdnavi --print <category> <item>`.\n{}",
        failures.join("\n")
    )))
}

#[cfg(windows)]
pub fn copy(text: &str) -> Result<()> {
    windows_copy(text)
}

fn windows_copy(text: &str) -> Result<()> {
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
    let mut stdin = child.stdin.take().unwrap();
    let bytes = text.as_bytes().to_vec();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = stdin.write_all(&bytes);
        drop(stdin);
        let _ = sender.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            receiver
                .recv_timeout(std::time::Duration::from_millis(100))
                .map_err(|_| Error("Clipboard input could not be sent.".into()))??;
            if status.success() {
                return Ok(());
            }
            return Err(Error("Clipboard helper failed.".into()));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error("Clipboard helper timed out after 3 seconds.".into()));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
