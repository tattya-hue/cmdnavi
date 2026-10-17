//! Interactive line editing is explicitly enabled by the executable, never by unit-test stdin.
use crate::completion::{self, Candidate, Context};
use crate::{Error, Result};
use rustyline::{
    Editor, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::MemHistory,
    validate::Validator,
};
use std::io::{BufRead, Write};

struct Suggestions(Vec<Candidate>);
impl Helper for Suggestions {}
impl Highlighter for Suggestions {}
impl Hinter for Suggestions {
    type Hint = String;
}
impl Validator for Suggestions {}
impl Completer for Suggestions {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        if pos != line.len() {
            return Ok((pos, Vec::new()));
        }
        Ok((
            0,
            self.0
                .iter()
                .filter(|c| c.value.starts_with(line))
                .map(|c| Pair {
                    display: format!("{}  {}", visible(&c.value), visible(&c.description)),
                    replacement: c.value.clone(),
                })
                .collect(),
        ))
    }
}

pub fn visible(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub struct Session<'a> {
    pub input: &'a mut dyn BufRead,
    pub output: &'a mut dyn Write,
    pub terminal: bool,
}
impl<'a> Session<'a> {
    pub fn new(input: &'a mut dyn BufRead, output: &'a mut dyn Write, terminal: bool) -> Self {
        Self {
            input,
            output,
            terminal,
        }
    }
    pub fn ask(
        &mut self,
        label: &str,
        initial: &str,
        context: Context,
        data: &crate::model::Config,
    ) -> Result<Option<String>> {
        let value = if self.terminal {
            self.output.flush()?;
            let config = rustyline::Config::builder()
                .completion_type(rustyline::CompletionType::List)
                .build();
            let mut editor =
                Editor::<Suggestions, MemHistory>::with_history(config, MemHistory::new())
                    .map_err(|e| Error(e.to_string()))?;
            let suggestions = completion::candidates(data, &context, "");
            for item in &suggestions {
                let _ = editor.add_history_entry(&item.value);
            }
            editor.set_helper(Some(Suggestions(suggestions)));
            match editor.readline_with_initial(label, (initial, "")) {
                Ok(value) => value,
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                    writeln!(self.output, "Cancelled.")?;
                    return Ok(None);
                }
                Err(error) => return Err(Error(format!("Input could not be read: {error}"))),
            }
        } else {
            write!(self.output, "{label}")?;
            self.output.flush()?;
            crate::cli::read_line(self.input)?
        };
        if value.trim() == "/cancel" {
            writeln!(self.output, "Cancelled.")?;
            return Ok(None);
        }
        if value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        {
            return Err(Error("Input contains terminal control characters. Nothing was saved; use normal text or paste the command again.".into()));
        }
        Ok(Some(value))
    }
    pub fn plain(&mut self, label: &str) -> Result<Option<String>> {
        self.ask(label, "", Context::None, &Default::default())
    }
    pub fn confirm(&mut self, label: &str) -> Result<bool> {
        Ok(self
            .plain(label)?
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "y" | "yes")))
    }
    pub fn finish_form(&mut self) -> Result<()> {
        if !self.terminal && !self.input.fill_buf()?.is_empty() {
            return Err(Error("Unexpected extra input. Nothing was saved. For a multiline command, use `cmdnavi add <category> \"command\" \"description\"` or YAML editing.".into()));
        }
        Ok(())
    }
}
