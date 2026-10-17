//! Keyboard-driven browser. Stored command strings are never executed.
use crate::model::{CommandEntry, Config};
use crate::{Result, clipboard, config, interaction};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};
use std::path::Path;
use unicode_width::UnicodeWidthChar;

struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let screen = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            cursor::Hide,
            event::EnableBracketedPaste
        )?;
        Ok(screen)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            SetAttribute(Attribute::Reset),
            ResetColor,
            event::DisableBracketedPaste,
            cursor::Show,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[derive(Default)]
struct State {
    query: String,
    selected: usize,
    detail_offset: usize,
    message: String,
}

fn matches<'a>(data: &'a Config, query: &str) -> Vec<(&'a str, usize, &'a CommandEntry)> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    data.iter()
        .flat_map(|(category, entries)| {
            entries
                .iter()
                .enumerate()
                .map(move |(index, entry)| (category.as_str(), index, entry))
        })
        .filter(|(category, _, entry)| {
            let haystack =
                format!("{category}\n{}\n{}", entry.command, entry.description).to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .collect()
}

pub fn run(path: &Path) -> Result<()> {
    let mut data = config::load(path)?;
    let mut state = State::default();
    let mut screen = Screen::enter()?;
    loop {
        let results = matches(&data, &state.query);
        state.selected = state.selected.min(results.len().saturating_sub(1));
        render(&data, &results, &mut state)?;
        let key = match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            Event::Paste(text) => {
                state.query.extend(text.chars().filter(|c| !c.is_control()));
                state.selected = 0;
                state.detail_offset = 0;
                continue;
            }
            _ => continue,
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => break,
            KeyCode::Char('a' | 'e') if ctrl => {
                let editing = key.code == KeyCode::Char('e');
                let selected = results
                    .get(state.selected)
                    .map(|(category, index, _)| (category.to_string(), *index));
                if editing && selected.is_none() {
                    continue;
                }
                drop(screen);
                let stdin = io::stdin();
                let stdout = io::stdout();
                let mut input = stdin.lock();
                let mut output = stdout.lock();
                let result = if editing {
                    let (category, index) = selected.unwrap();
                    interaction::edit_entry(
                        path,
                        &category,
                        index,
                        &data[&category],
                        &mut input,
                        &mut output,
                    )
                } else {
                    interaction::choose_category(path, &mut input, &mut output).and_then(
                        |category| {
                            if let Some(category) = category {
                                interaction::add(path, &category, &mut input, &mut output)
                            } else {
                                Ok(())
                            }
                        },
                    )
                };
                if let Err(error) = &result {
                    writeln!(output, "{error}")?;
                }
                write!(output, "Press Enter to return to the browser: ")?;
                output.flush()?;
                let _ = crate::cli::read_line(&mut input)?;
                drop(output);
                drop(input);
                data = config::load(path)?;
                state.message = if result.is_ok() {
                    "Ready.".into()
                } else {
                    "No changes saved. Try again with Ctrl+E or Ctrl+A.".into()
                };
                screen = Screen::enter()?;
            }
            KeyCode::Enter => {
                if let Some((_, _, entry)) = results.get(state.selected) {
                    state.message = match clipboard::copy(&entry.command) {
                        Ok(()) => "Copied command to clipboard.".into(),
                        Err(error) => error.to_string(),
                    };
                }
            }
            KeyCode::Up => {
                state.selected = state.selected.saturating_sub(1);
                state.detail_offset = 0;
            }
            KeyCode::Down => {
                state.selected = (state.selected + 1).min(results.len().saturating_sub(1));
                state.detail_offset = 0;
            }
            KeyCode::PageUp => state.detail_offset = state.detail_offset.saturating_sub(3),
            KeyCode::PageDown => state.detail_offset = state.detail_offset.saturating_add(3),
            KeyCode::Char('u') if ctrl => {
                state.query.clear();
                state.selected = 0;
                state.detail_offset = 0;
            }
            KeyCode::Backspace => {
                state.query.pop();
                state.selected = 0;
                state.detail_offset = 0;
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                state.query.push(c);
                state.selected = 0;
                state.detail_offset = 0;
            }
            _ => {}
        }
    }
    Ok(())
}

// Strip controls so pasted/stored text cannot move the terminal cursor.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn clip(text: &str, width: usize) -> String {
    let text = clean(text);
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= width
        })
        .collect()
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let mut used = 0;
        for c in clean(line).chars() {
            let size = c.width().unwrap_or(0);
            if used + size > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(c);
            used += size;
        }
        lines.push(current);
    }
    lines
}

fn row(
    output: &mut impl Write,
    y: usize,
    text: &str,
    width: usize,
    accent: bool,
    color: bool,
) -> Result<()> {
    queue!(output, cursor::MoveTo(0, y as u16))?;
    if accent {
        queue!(output, SetAttribute(Attribute::Bold))?;
        if color {
            queue!(output, SetForegroundColor(Color::Cyan))?;
        }
    }
    queue!(
        output,
        Print(clip(text, width)),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    Ok(())
}

fn render(
    data: &Config,
    results: &[(&str, usize, &CommandEntry)],
    state: &mut State,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = usize::from(width).saturating_sub(1);
    let height = usize::from(height);
    let color = std::env::var_os("NO_COLOR").is_none();
    let mut output = io::stdout().lock();
    queue!(output, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    if height < 12 || width < 35 {
        row(
            &mut output,
            0,
            "Enlarge terminal (36 x 12). Esc exits.",
            width,
            false,
            color,
        )?;
        output.flush()?;
        return Ok(());
    }
    let total: usize = data.values().map(Vec::len).sum();
    row(
        &mut output,
        0,
        &format!(
            "cmdnavi   |   {} categories   ·   {total} commands",
            data.len()
        ),
        width,
        true,
        color,
    )?;
    row(
        &mut output,
        2,
        &format!("Search > {}", state.query),
        width,
        true,
        color,
    )?;
    row(
        &mut output,
        3,
        &format!("{} matches · type to filter · Ctrl+U clears", results.len()),
        width,
        false,
        color,
    )?;
    let list_height = (height.saturating_sub(10) / 2).max(1);
    let start = state.selected.saturating_sub(list_height - 1);
    if results.is_empty() {
        row(
            &mut output,
            4,
            if total == 0 {
                "No commands yet. Ctrl+A to add your first one."
            } else {
                "No matches. Shorten the search or Ctrl+U to clear."
            },
            width,
            false,
            color,
        )?;
    }
    for (i, (category, index, entry)) in results.iter().enumerate().skip(start).take(list_height) {
        let marker = if i == state.selected { ">" } else { " " };
        row(
            &mut output,
            4 + i - start,
            &format!("{marker} [{category} #{}] {}", index + 1, entry.command),
            width,
            i == state.selected,
            color,
        )?;
    }
    let detail_start = 5 + list_height;
    let detail_height = height.saturating_sub(detail_start + 4);
    if let Some((category, index, entry)) = results.get(state.selected) {
        let detail = wrap(
            &format!(
                "{}\n\n{}",
                entry.command,
                if entry.description.is_empty() {
                    "No description yet. Ctrl+E to add one."
                } else {
                    &entry.description
                }
            ),
            width,
        );
        state.detail_offset = state
            .detail_offset
            .min(detail.len().saturating_sub(detail_height));
        row(
            &mut output,
            detail_start - 1,
            &format!(
                "── {category} #{} · detail {}/{} · PgUp/PgDn scroll",
                index + 1,
                state.detail_offset + 1,
                detail.len()
            ),
            width,
            true,
            color,
        )?;
        for (i, line) in detail
            .iter()
            .skip(state.detail_offset)
            .take(detail_height)
            .enumerate()
        {
            row(&mut output, detail_start + i, line, width, false, color)?;
        }
    }
    // Reserve two rows for status, so clipboard setup guidance remains visible.
    for (i, line) in wrap(&state.message, width).iter().take(2).enumerate() {
        row(&mut output, height - 4 + i, line, width, false, color)?;
    }
    row(
        &mut output,
        height - 2,
        "↑↓ Select · Enter Copy · Ctrl+A Add · Ctrl+E Edit",
        width,
        true,
        color,
    )?;
    row(
        &mut output,
        height - 1,
        "Esc Quit · Ctrl+U Clear · PgUp/PgDn Detail",
        width,
        false,
        color,
    )?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_combines_terms_across_category_command_and_description() {
        let data = Config::from([(
            "network".into(),
            vec![CommandEntry {
                command: "IP addr".into(),
                description: "アドレス確認".into(),
            }],
        )]);
        assert_eq!(matches(&data, "NETWORK ip アドレス").len(), 1);
        assert!(matches(&data, "network docker").is_empty());
        assert_eq!(matches(&data, "").len(), 1);
    }
    #[test]
    fn unicode_layout_and_control_characters_stay_inside_rows() {
        assert_eq!(clip("日本語abc", 7), "日本語a");
        assert_eq!(wrap("日本語abc", 4), ["日本", "語ab", "c"]);
        assert_eq!(clip("a\x1b\nb", 9), "a  b");
    }
}
