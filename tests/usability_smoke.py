"""Linux PTY regression checks for editing, paste, context completion and shell integration.
Run after cargo build: python3 tests/usability_smoke.py [path/to/cmdnavi].
"""
import fcntl
import os
from pathlib import Path
import pty
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / 'target/debug/cmdnavi'


class Terminal:
    def __init__(self, args, env):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 100, 0, 0))
        self.before = termios.tcgetattr(self.slave)
        def setup():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        self.process = subprocess.Popen(args, stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env, preexec_fn=setup)
        self.buffer = b''

    def send(self, text):
        os.write(self.master, text.encode())

    def expect(self, text, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if text.encode() in self.buffer:
                end = self.buffer.index(text.encode()) + len(text.encode())
                chunk, self.buffer = self.buffer[:end], self.buffer[end:]
                return chunk
            if select.select([self.master], [], [], .1)[0]:
                self.buffer += os.read(self.master, 65536)
        raise AssertionError(f'Missing {text!r}: {self.buffer.decode(errors="replace")}')

    def finish(self):
        assert self.process.wait(timeout=5) == 0
        assert termios.tcgetattr(self.slave) == self.before, 'terminal mode not restored'
        self.close()

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()
        os.close(self.master)
        os.close(self.slave)


def run(root):
    env = dict(os.environ, XDG_CONFIG_HOME=str(root), TERM='xterm-256color', NO_COLOR='1', PATH=str(BINARY.parent) + os.pathsep + os.environ['PATH'])
    env.pop('WSL_INTEROP', None)
    env.pop('WSL_DISTRO_NAME', None)
    def cli(*args):
        return subprocess.check_output([BINARY, *args], env=env, text=True)
    def entry(category):
        return cli('--print', category, '1')
    cli('add', 'network', 'ip addr', 'IPアドレス')
    cli('add', 'git', 'git status')
    sentinel = root / 'never-evaluated'
    dangerous = f"printf '%s' \"$(touch {sentinel})\""
    cli('add', 'shell', dangerous)

    # A category field must complete categories, and only a command field completes commands.
    t = Terminal([BINARY, 'add'], env)
    try:
        t.expect('Category (name')
        t.send('net\t\n')
        t.expect('Command:')
        t.send('ip\t\x1b[DZ\n')  # Complete ip addr, move left, then insert Z.
        t.expect('Description (optional')
        t.send('test\n')
        t.finish()
    except BaseException:
        t.close()
        raise
    assert cli('--print', 'network', '2') == 'ip addZr'

    # Bracketed multiline paste stays in the command field until Enter accepts it.
    multiline = 'echo first\necho 日本語'
    t = Terminal([BINARY, 'add', 'multiline'], env)
    try:
        t.expect('Command:')
        t.send('\x1b[200~' + multiline + '\x1b[201~\n')
        t.expect('Description (optional')
        t.send('correct description\n')
        t.finish()
    except BaseException:
        t.close()
        raise
    assert entry('multiline') == multiline
    assert 'correct description' in cli('--show', 'multiline')

    # Existing command values are prefilled and can be edited in place.
    t = Terminal([BINARY, 'edit', 'multiline', '1'], env)
    try:
        t.expect('New command:')
        t.send('\x15echo changed\n')
        t.expect('New description')
        t.send('\x15changed description\n')
        t.finish()
    except BaseException:
        t.close()
        raise
    assert entry('multiline') == 'echo first\necho changed', repr(entry('multiline'))

    # Ctrl+C cancels a form without creating a partial entry.
    t = Terminal([BINARY, 'add', 'cancelled'], env)
    try:
        t.expect('Command:')
        t.send('some text\x03')
        t.finish()
    except BaseException:
        t.close()
        raise
    assert 'cancelled' not in cli('list').splitlines()

    t = Terminal([BINARY], env)
    try:
        t.expect('F1 Help')
        t.send('ipZ\x1b[D\x1b[3~')  # Move left and Delete; query becomes ip.
        t.expect('Search > ip')
        cli('add', 'network', 'ip link', 'external update')
        t.expect('3 matches')
        # A slow helper cannot trap the browser; quitting is immediate.
        helper = root / 'wl-copy'
        helper.write_text('#!/bin/sh\nsleep 4\ncat >/dev/null\n')
        helper.chmod(0o755)
        # A separate browser process below supplies the clipboard environment.
        t.send('\x1b')
        t.finish()
    except BaseException:
        t.close()
        raise

    clipboard_env = dict(env, WAYLAND_DISPLAY='test', PATH=str(root)+os.pathsep+env['PATH'])
    t = Terminal([BINARY], clipboard_env)
    try:
        t.expect('F1 Help')
        t.send('\r')
        t.expect('Copying')
        start = time.monotonic()
        t.send('\x1b')
        t.finish()
        assert time.monotonic()-start < 2
    except BaseException:
        t.close()
        raise

    # Check actual Bash Readline insertion, without executing the resulting command.
    t = Terminal(['bash', '--noprofile', '--norc'], env)
    try:
        t.send("PS1=$'\\x1eREADY> \\x1f'; source " + str(ROOT/'completions/cmdnavi.bash') + "\n")
        t.expect('\x1eREADY> \x1f')
        t.send("_capture(){ printf '\\nCAPTURE:%s:END\\x1f\\n' \"$READLINE_LINE\"; READLINE_LINE=; READLINE_POINT=0; }; bind -x '\"\\C-x\":_capture'\n")
        t.expect('\x1eREADY> \x1f')
        for typed, expected in [('cmdnavi a\t', 'cmdnavi add '), ('cmdnavi edit net\t', 'cmdnavi edit network '), ('cmdnavi --print network 1\t', 'cmdnavi --print network 1 '), ('cmdnavi --print network ip\\ addr\t', 'cmdnavi --print network '), ('cmdnavi --print network \"ip addr\t', 'cmdnavi --print network '), ("cmdnavi --print network 'ip addr\t", 'cmdnavi --print network ')] :
            t.send(typed + '\x18')
            capture = t.expect(':END\x1f').decode(errors='replace')
            line = capture.rsplit('CAPTURE:', 1)[-1].split(':END')[0]
            assert line.startswith(expected), repr(line)
            if 'ip\\ addr' in typed or '"ip addr' in typed or "'ip addr" in typed:
                # The inserted command must remain a single shell argument.
                decoded = subprocess.check_output(['bash', '-c', "printf '%s\\0' " + line], env=env).split(b'\0')
                assert decoded[3:-1] == [b'ip addr'], (line, decoded)
            t.expect('\x1eREADY> \x1f')
        for prefix in ['printf', '"printf', "'printf"]:
            t.send('cmdnavi --print shell ' + prefix + '\t\x18')
            capture = t.expect(':END\x1f').decode(errors='replace')
            line = capture.rsplit('CAPTURE:', 1)[-1].split(':END')[0]
            decoded = subprocess.check_output(['bash', '-c', "printf '%s\\0' " + line], env=env).split(b'\0')
            assert decoded[3:-1] == [dangerous.encode()], (line,decoded)
            assert not sentinel.exists()
            t.expect('\x1eREADY> \x1f')
        t.send('exit\n')
        t.finish()
    except BaseException:
        t.close()
        raise



    fish = os.environ.get('CMDNAVI_TEST_FISH') or shutil.which('fish')
    if fish:
        source = "source " + str(ROOT/'completions/cmdnavi.fish') + "; "
        for line, expected, forbidden in [
            ('cmdnavi a', 'add', 'ip addr'),
            ('cmdnavi edit net', 'network', 'ip addr'),
            ('cmdnavi --print network ip\\ addr', 'ip addr', 'git status'),
            ('cmdnavi add network "ip addr" ', None, 'ip addr'),
            ('cmdnavi --search ', None, 'ip addr'),
        ]:
            expression = source + "complete -C '" + line.replace("'", "\\'") + "'"
            result = subprocess.check_output([fish,'--no-config','-c',expression], env=env, text=True)
            values = [v.split('\t',1)[0] for v in result.splitlines()]
            if expected is not None: assert expected in values, (line,values)
            assert forbidden not in values, (line,values)

    if shutil.which('zsh'):
        t = Terminal(['zsh', '-f', '-d'], env)
        try:
            t.send("autoload -Uz compinit; compinit -D -u; PS1=$'\\x1eREADY> \\x1f'; source " + str(ROOT/'completions/_cmdnavi') + "\n")
            t.expect('\x1eREADY> \x1f')
            t.send("_capture(){ printf '\\nCAPTURE:%s:END\\x1f\\n' \"$BUFFER\"; BUFFER=; CURSOR=0; zle reset-prompt; }; zle -N _capture; bindkey '^X' _capture\n")
            t.expect('\x1eREADY> \x1f')
            for typed, expected in [('cmdnavi a\t', 'cmdnavi add '), ('cmdnavi edit net\t', 'cmdnavi edit network '), ('cmdnavi --print network ip\\ addr\t', 'cmdnavi --print network '), ('cmdnavi --print network \"ip addr\t', 'cmdnavi --print network '), ("cmdnavi --print network 'ip addr\t", 'cmdnavi --print network '), ('cmdnavi --search ip\t', 'cmdnavi --search ip')]:
                t.send(typed + '\x18')
                capture = t.expect(':END\x1f').decode(errors='replace')
                line = capture.rsplit('CAPTURE:', 1)[-1].split(':END')[0]
                assert line.startswith(expected), repr(line)
                if 'ip\\ addr' in typed or '"ip addr' in typed or "'ip addr" in typed:
                    decoded = subprocess.check_output(['zsh', '-c', "printf '%s\\0' " + line], env=env).split(b'\0')
                    assert decoded[3:-1] == [b'ip addr'], (line, decoded)
                t.expect('\x1eREADY> \x1f')
            for prefix in ['printf', '"printf', "'printf"]:
                t.send('cmdnavi --print shell ' + prefix + '\t\x18')
                capture = t.expect(':END\x1f').decode(errors='replace')
                line = capture.rsplit('CAPTURE:', 1)[-1].split(':END')[0]
                decoded = subprocess.check_output(['zsh', '-c', "printf '%s\\0' " + line], env=env).split(b'\0')
                assert decoded[3:-1] == [dangerous.encode()], (line,decoded)
                assert not sentinel.exists()
                t.expect('\x1eREADY> \x1f')
            t.send('exit\n')
            t.finish()
        except BaseException:
            t.close()
            raise


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='cmdnavi-usability-test-') as directory:
        run(Path(directory))
    print('Usability PTY checks passed: scoped completion, cursor editing, multiline paste, prefill, cancel, refresh, responsive copying, Bash/Zsh insertion.')
