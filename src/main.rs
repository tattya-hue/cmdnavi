use std::io::{self, IsTerminal, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let metadata = cmdnavi::cli::run_metadata(&args, &mut io::stdout().lock());
    let result = metadata.unwrap_or_else(|| run(&args));
    if let Err(error) = result {
        print_error(&error);
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> cmdnavi::Result<()> {
    let path = cmdnavi::config::config_path()?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let terminal = stdin.is_terminal()
        && stdout.is_terminal()
        && std::env::var("TERM").as_deref() != Ok("dumb");
    if args == ["--browse"] || (args.is_empty() && terminal) {
        if !terminal {
            return Err(cmdnavi::Error("Interactive browsing needs a terminal. Use `cmdnavi --search <text>` or `cmdnavi <category>` instead.".into()));
        }
        return cmdnavi::browser::run(&path);
    }
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    cmdnavi::cli::run_with_terminal(args, &path, &mut input, &mut output, terminal)
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
