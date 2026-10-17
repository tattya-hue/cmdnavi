"""Linux PTY smoke test: cargo build && python3 tests/terminal_smoke.py.

Uses a temporary configuration and fake clipboard helper, never the user's data.
"""
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time

BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / "target/debug/cmdnavi"


def check_browser(root):
    env = dict(os.environ, XDG_CONFIG_HOME=str(root), TERM="xterm-256color", NO_COLOR="1",
               WAYLAND_DISPLAY="test", PATH=str(root) + os.pathsep + os.environ["PATH"])
    env.pop("DISPLAY", None)
    env.pop("WSL_INTEROP", None)
    env.pop("WSL_DISTRO_NAME", None)
    copied = root / "copied"
    env["CMDNAVI_TEST_CLIPBOARD"] = str(copied)
    helper = root / "wl-copy"
    helper.write_text('#!/bin/sh\ncat > "$CMDNAVI_TEST_CLIPBOARD"\n')
    helper.chmod(0o755)
    # Shell metacharacters must be copied literally, never executed.
    command = f"echo 日本語; touch {root / 'must-not-exist'}"
    subprocess.run([BINARY, "add", "network", command, "アドレス確認"], env=env, check=True, capture_output=True)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
    before = termios.tcgetattr(slave)

    def setup():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    process = subprocess.Popen([BINARY], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=setup)
    transcript = bytearray()

    def expect(text):
        chunk = bytearray()
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                data = os.read(master, 65536)
                chunk.extend(data)
                transcript.extend(data)
                if text.encode() in chunk:
                    return
            if process.poll() is not None:
                break
        raise AssertionError(f"Missing {text!r}: {chunk.decode(errors='replace')}")

    def send(text):
        os.write(master, text.encode())

    try:
        expect("Ctrl+E Edit")
        send("c")
        expect("Search > c")
        assert not copied.exists(), "Typing c in search unexpectedly copied a command"
        send("\x15")
        expect("1 matches")
        send("missing")
        expect("No matches")
        send("\x15")  # Ctrl+U
        expect("1 matches")
        send("アドレス")
        expect("1 matches")
        send("\r")
        expect("Command details")
        assert not copied.exists(), "Opening details unexpectedly changed the clipboard"
        send("\r")
        expect("c Copy · e Edit · d Remove")
        assert not copied.exists(), "Enter in details unexpectedly changed the clipboard"
        send("?")
        expect("Command details · keyboard help")
        send("\x1b")
        expect("c Copy")
        send("c")  # Details accept a single key; search fields do not.
        expect("Copied command")
        assert copied.read_text() == command
        assert not (root / "must-not-exist").exists()
        helper.write_text("#!/bin/sh\nexit 1\n")
        send("\x19")
        expect("Could not copy")
        send("\x1b")  # close full error message
        expect("c Copy")
        send("e")  # Details open a separate editing form.
        expect("New command:")
        send("\x15cmed\n")  # Action letters must remain ordinary text in the form.
        expect("New description")
        send("\x15updated description\n")
        expect("Search cleared")
        output = subprocess.check_output([BINARY, "--print", "network", "1"], env=env, text=True)
        assert output.strip() == "cmed", "Editing interpreted action letters as shortcuts"
        expect_text = "updated description"
        send("\x15")
        expect(expect_text)
        send("\x01")  # Ctrl+A
        expect("Category (name")
        send("#1\n")
        expect("Command:")
        send("pwd\n")
        expect("Description (optional")
        send("\n")
        expect("2 matches")
        send("\x1b[B")  # Down
        expect("No description yet")
        send("\r")
        expect("d Remove")
        send("d")
        expect('Remove "pwd"?')
        send("n\n")
        expect("No changes.")
        assert "pwd" in subprocess.check_output([BINARY, "network"], env=env, text=True)
        send("\r")
        expect("d Remove")
        send("d")
        expect('Remove "pwd"?')
        send("y\n")
        expect("1 matches")
        send("\r")
        expect("u Undo")
        send("u")
        expect("Last change undone.")
        assert "pwd" in subprocess.check_output([BINARY, "network"], env=env, text=True)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 25, 0, 0))
        # Trigger redraw even on systems that don't deliver SIGWINCH for this PTY.
        send("\x15")
        expect("Enlarge terminal")
        send("\x1b")
        assert process.wait(timeout=5) == 0
        assert termios.tcgetattr(slave) == before, "Terminal mode was not restored"
        assert b"\x1b[36m" not in transcript, "NO_COLOR ignored"
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
        os.close(slave)


with tempfile.TemporaryDirectory(prefix="cmdnavi-terminal-test-") as directory:
    check_browser(Path(directory))
print("PTY smoke test passed: filter, copy, clipboard failure, edit, add, navigation, resize, terminal restoration.")
