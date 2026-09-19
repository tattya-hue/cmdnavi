use std::io::{self, IsTerminal, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = cmdnavi::config::config_path().and_then(|path| {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let stdout = io::stdout();
        let mut output = stdout.lock();
        cmdnavi::cli::run(&args, &path, &mut input, &mut output)
    });
    if let Err(error) = result {
        print_error(&error);
        std::process::exit(1);
    }
}

fn print_error(error: &cmdnavi::Error) {
    let stderr = io::stderr();
    let color = stderr.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut stderr = stderr.lock();
    if color {
        let _ = writeln!(stderr, "\x1b[1;31mError:\x1b[0m {error}");
    } else {
        let _ = writeln!(stderr, "Error: {error}");
    }
}
