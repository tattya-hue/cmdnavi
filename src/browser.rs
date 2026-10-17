//! Keyboard-driven browser. Stored commands are data, never executable code.
use crate::completion::Context;
use crate::model::{CommandEntry, Config, matches_query};
use crate::session::Session;
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
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) struct Screen;
impl Screen {
    pub(crate) fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let screen = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
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
    cursor: usize,
    selected: usize,
    detail_offset: usize,
    message: String,
    help: bool,
    help_offset: usize,
    viewing: bool,
}
impl State {
    fn insert(&mut self, value: &str) {
        self.query.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.changed();
    }
    fn previous(&self) -> usize {
        self.query[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.query[self.cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.query.len())
    }
    fn changed(&mut self) {
        self.selected = 0;
        self.detail_offset = 0;
        self.viewing = false;
    }
    fn clear(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.changed();
    }
    fn remove_word(&mut self) {
        let prefix = self.query[..self.cursor].trim_end_matches(char::is_whitespace);
        let start = prefix
            .rfind(char::is_whitespace)
            .map(|i| i + prefix[i..].chars().next().unwrap().len_utf8())
            .unwrap_or(0);
        self.query.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.changed();
    }
}
fn matches<'a>(data: &'a Config, query: &str) -> Vec<(&'a str, usize, &'a CommandEntry)> {
    data.iter()
        .flat_map(|(category, entries)| {
            entries
                .iter()
                .enumerate()
                .map(move |(i, e)| (category.as_str(), i, e))
        })
        .filter(|(category, _, entry)| matches_query(category, entry, query))
        .collect()
}
fn stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}
fn reload(path: &Path, data: &mut Config, state: &mut State, message: Option<&str>) -> bool {
    let selected = matches(data, &state.query)
        .get(state.selected)
        .map(|(c, _, e)| (c.to_string(), e.command.clone()));
    match config::load(path) {
        Ok(new) => {
            *data = new;
            if let Some(message) = message {
                state.message = message.into();
            }
            if let Some((category, command)) = selected
                && let Some(i) = matches(data, &state.query)
                    .iter()
                    .position(|(c, _, e)| *c == category && e.command == command)
            {
                state.selected = i;
            } else {
                state.viewing = false;
            }
            true
        }
        Err(error) => {
            state.message = format!("Reload failed. Keeping the last readable data.\n{error}");
            state.help = true;
            state.help_offset = 0;
            false
        }
    }
}

pub fn run(path: &Path) -> Result<()> {
    run_screen(config::load(path)?, Some(path), None).map(|_| ())
}

pub(crate) fn choose_entry(
    category: &str,
    entries: &[CommandEntry],
    title: &str,
) -> Result<Option<usize>> {
    let data = Config::from([(category.into(), entries.to_vec())]);
    run_screen(data, None, Some(title)).map(|selected| selected.map(|(_, index)| index))
}

fn run_screen(
    mut data: Config,
    path: Option<&Path>,
    selection_title: Option<&str>,
) -> Result<Option<(String, usize)>> {
    let selecting = selection_title.is_some();
    let mut known_stamp = path.and_then(stamp);
    let mut state = State::default();
    let mut screen = Screen::enter()?;
    let mut copying: Option<std::sync::mpsc::Receiver<Result<()>>> = None;
    let mut dirty = true;
    loop {
        if let Some(path) = path
            && known_stamp != stamp(path)
        {
            reload(path, &mut data, &mut state, None);
            known_stamp = stamp(path);
            dirty = true;
        }
        if let Some(receiver) = &copying
            && let Ok(result) = receiver.try_recv()
        {
            match result {
                Ok(()) => state.message = "Copied command to clipboard.".into(),
                Err(error) => {
                    state.message = error.to_string();
                    state.help = true;
                    state.help_offset = 0;
                }
            }
            copying = None;
            dirty = true;
        }
        let results = matches(&data, &state.query);
        state.selected = state.selected.min(results.len().saturating_sub(1));
        if dirty {
            if results.is_empty() {
                state.viewing = false;
            }
            render(&data, &results, &mut state, selection_title)?;
            dirty = false;
        }
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        dirty = true;
        let mut key = match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            Event::Paste(text) if !state.help && !state.viewing => {
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                state.insert(&text);
                continue;
            }
            _ => continue,
        };
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('j' | 'm'))
        {
            key.code = KeyCode::Enter;
            key.modifiers = KeyModifiers::NONE;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if state.help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter => state.help = false,
                KeyCode::Char('c') if ctrl => break,
                KeyCode::Up => state.help_offset = state.help_offset.saturating_sub(1),
                KeyCode::Down => state.help_offset = state.help_offset.saturating_add(1),
                KeyCode::PageUp => state.help_offset = state.help_offset.saturating_sub(8),
                KeyCode::PageDown => state.help_offset = state.help_offset.saturating_add(8),
                _ => {}
            }
            continue;
        }
        let (width, height) = terminal::size()?;
        if width < 36 || height < 12 {
            if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
                break;
            }
            continue;
        }
        // Details have no text input: offer single-key actions there only.
        // Search and form fields must keep these letters as ordinary input.
        if state.viewing && key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            match key.code {
                KeyCode::Char('c') => {
                    key.code = KeyCode::Char('y');
                    key.modifiers = KeyModifiers::CONTROL;
                }
                KeyCode::Char('a' | 'e' | 'd' | 'n' | 'r') => {
                    key.modifiers = KeyModifiers::CONTROL;
                }
                KeyCode::Char('m') => {
                    key.code = KeyCode::Char('g');
                    key.modifiers = KeyModifiers::CONTROL;
                }
                KeyCode::Char('u') => {
                    key.code = KeyCode::Char('z');
                    key.modifiers = KeyModifiers::CONTROL;
                }
                KeyCode::Char('?') => key.code = KeyCode::F(1),
                _ => {}
            }
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if state.viewing {
            match key.code {
                KeyCode::Esc => {
                    state.viewing = false;
                    state.detail_offset = 0;
                    continue;
                }
                KeyCode::Up => {
                    state.detail_offset = state.detail_offset.saturating_sub(1);
                    continue;
                }
                KeyCode::Down => {
                    state.detail_offset = state.detail_offset.saturating_add(1);
                    continue;
                }
                KeyCode::PageUp => {
                    state.detail_offset = state.detail_offset.saturating_sub(8);
                    continue;
                }
                KeyCode::PageDown => {
                    state.detail_offset = state.detail_offset.saturating_add(8);
                    continue;
                }
                _ if !ctrl && key.code != KeyCode::F(1) => continue,
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => break,
            KeyCode::F(1) => {
                state.help = true;
                state.help_offset = 0;
            }
            KeyCode::Char('r') if ctrl && !selecting => {
                let path = path.expect("browse path");
                reload(path, &mut data, &mut state, Some("Reloaded."));
                known_stamp = stamp(path);
            }
            KeyCode::Char('z') if ctrl && !selecting => {
                let path = path.expect("browse path");
                match config::undo(path) {
                    Ok(()) => {
                        reload(path, &mut data, &mut state, Some("Last change undone."));
                    }
                    Err(error) => {
                        state.message = error.to_string();
                        state.help = true;
                        state.help_offset = 0;
                    }
                }
            }
            KeyCode::Char('a' | 'e' | 'd' | 'g' | 'n') if ctrl && !selecting => {
                let path = path.expect("browse path");
                state.viewing = false;
                let selected = results
                    .get(state.selected)
                    .map(|(c, i, _)| (c.to_string(), *i));
                if key.code != KeyCode::Char('a') && selected.is_none() {
                    state.message = "Select a command first, or Ctrl+A to add one.".into();
                    continue;
                }
                let before = data.clone();
                drop(screen);
                let stdin = io::stdin();
                let stdout = io::stdout();
                let mut input = stdin.lock();
                let mut output = stdout.lock();
                let mut session = Session::new(&mut input, &mut output, true);
                let result = perform_action(path, key.code, selected, &data, &mut session);
                drop(output);
                drop(input);
                let reloaded = reload(path, &mut data, &mut state, None);
                known_stamp = stamp(path);
                match result {
                    _ if !reloaded => {}
                    Ok(()) => {
                        let changed = before != data;
                        state.message = if changed {
                            "Saved. Ctrl+Z undoes the last change."
                        } else {
                            "No changes."
                        }
                        .into();
                        if changed
                            && !state.query.is_empty()
                            && matches(&data, &state.query).is_empty()
                        {
                            state.clear();
                            state.message =
                                "Search cleared to show saved changes. Ctrl+Z undoes changes."
                                    .into();
                        }
                    }
                    Err(error) => {
                        state.message = error.to_string();
                        state.help = true;
                        state.help_offset = 0;
                    }
                }
                screen = Screen::enter()?;
            }
            KeyCode::Enter => {
                if selecting {
                    if let Some((category, index, _)) = results.get(state.selected) {
                        return Ok(Some(((*category).into(), *index)));
                    }
                } else if !results.is_empty() {
                    state.viewing = true;
                    state.detail_offset = 0;
                }
            }
            KeyCode::Char('y') if ctrl && !selecting && copying.is_none() => {
                if let Some((_, _, entry)) = results.get(state.selected) {
                    let text = entry.command.clone();
                    let (sender, receiver) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = sender.send(clipboard::copy(&text));
                    });
                    copying = Some(receiver);
                    state.message = "Copying command text… You can keep browsing.".into();
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
            KeyCode::Left => state.cursor = state.previous(),
            KeyCode::Right => state.cursor = state.next(),
            KeyCode::Home => state.cursor = 0,
            KeyCode::End => state.cursor = state.query.len(),
            KeyCode::Char('u') if ctrl => state.clear(),
            KeyCode::Char('w') if ctrl => state.remove_word(),
            KeyCode::Backspace => {
                let previous = state.previous();
                state.query.replace_range(previous..state.cursor, "");
                state.cursor = previous;
                state.changed();
            }
            KeyCode::Delete => {
                state.query.replace_range(state.cursor..state.next(), "");
                state.changed();
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                state.insert(&c.to_string())
            }
            _ => {}
        }
    }
    Ok(None)
}

fn perform_action(
    path: &Path,
    key: KeyCode,
    selected: Option<(String, usize)>,
    data: &Config,
    s: &mut Session<'_>,
) -> Result<()> {
    if key == KeyCode::Char('a') {
        if let Some(category) = interaction::choose_category_with(path, s)? {
            interaction::add_with(path, &category, s)?;
        }
        return Ok(());
    }
    let (category, index) = selected.unwrap();
    match key {
        KeyCode::Char('e') => {
            interaction::edit_entry_with(path, &category, index, &data[&category], s)
        }
        KeyCode::Char('d') => {
            interaction::remove_entry(path, &category, index, &data[&category], s)
        }
        KeyCode::Char('g') => {
            writeln!(s.output, "Move the selected command from [{category}].")?;
            if let Some(target) = interaction::choose_destination_with(path, &category, s)? {
                interaction::move_entry(path, &category, index, &data[&category], &target)?;
            }
            Ok(())
        }
        KeyCode::Char('n') => {
            if let Some(name) =
                s.ask("New category name: ", &category, Context::Categories, data)?
            {
                crate::cli::rename_category(path, &category, &name, &data[&category])?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
// Strip controls so pasted/stored text cannot move the terminal cursor.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub(crate) fn clip(text: &str, width: usize) -> String {
    let text = clean(text);
    let mut used = 0;
    text.graphemes(true)
        .take_while(|g| {
            used += g.width();
            used <= width
        })
        .collect()
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines() {
        let text = clean(line);
        let mut current = String::new();
        let mut used = 0;
        for grapheme in text.graphemes(true) {
            let size = grapheme.width();
            if used + size > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push_str(grapheme);
            used += size;
        }
        lines.push(current);
    }
    lines
}

pub(crate) fn row(
    output: &mut impl Write,
    y: usize,
    text: &str,
    width: usize,
    accent: bool,
    color: bool,
) -> Result<()> {
    result_row(output, y, text, width, accent, color, "")
}

fn result_row(
    output: &mut impl Write,
    y: usize,
    text: &str,
    width: usize,
    accent: bool,
    color: bool,
    query: &str,
) -> Result<()> {
    queue!(output, cursor::MoveTo(0, y as u16))?;
    if accent {
        queue!(output, SetAttribute(Attribute::Bold))?;
        if color {
            queue!(output, SetForegroundColor(Color::Cyan))?;
        }
    }
    let text = clip(text, width);
    let mut end = 0;
    for range in crate::model::match_ranges(&text, query) {
        queue!(
            output,
            Print(&text[end..range.start]),
            SetAttribute(Attribute::Underlined),
            Print(&text[range.clone()]),
            SetAttribute(Attribute::NoUnderline)
        )?;
        end = range.end;
    }
    queue!(
        output,
        Print(&text[end..]),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    Ok(())
}

fn render(
    data: &Config,
    results: &[(&str, usize, &CommandEntry)],
    state: &mut State,
    selection_title: Option<&str>,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = usize::from(width).saturating_sub(1);
    let height = usize::from(height);
    let color = std::env::var_os("NO_COLOR").is_none();
    let mut output = io::stdout().lock();
    queue!(output, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    if state.help && height >= 4 && width >= 10 {
        let text = if let Some(title) = selection_title {
            format!(
                "{title}\n\nType to filter by command text or description.\nUp/Down: select. Enter: choose.\nPgUp/PgDn: scroll the preview.\nCtrl+U: clear the search.\nEsc / Ctrl+C: cancel.\n"
            )
        } else if state.viewing {
            format!(
                "{}\n\nCommand details · keyboard help\nThis screen does not edit text.\nc: copy command text only (not its description).\ne: edit; d: remove (confirmation required); m: move.\na: add; n: rename category; u: undo the last change once.\nr: reload. ?: help.\nUp/Down, PgUp/PgDn: scroll.\nEsc: back to search. Ctrl+C: exit.\n\nThe Ctrl shortcuts from the search screen also work here.\nEditing opens a separate form: letters are ordinary text there.\n",
                state.message
            )
        } else {
            format!(
                "{}\n\nSearch · keyboard help\nType to search; Ctrl+W removes a word; Ctrl+U clears.\nUp/Down: select. Enter: open details. Esc / Ctrl+C: exit.\nLeft/Right, Home/End, Delete: edit search.\nPgUp/PgDn: scroll the preview.\nCtrl+A: add; Ctrl+E: edit; Ctrl+D: remove.\nCtrl+G: move; Ctrl+N: rename category.\nCtrl+Y: copy command text only.\nCtrl+Z: undo the last saved change once. Ctrl+R: reload.\n\nOpen details with Enter to use single-key actions: c copy, e edit, d remove, m move.\n\nCLI details: cmdnavi --help-all\n",
                state.message
            )
        };
        let lines = wrap(&text, width);
        let count = height.saturating_sub(2);
        state.help_offset = state.help_offset.min(lines.len().saturating_sub(count));
        for (i, line) in lines.iter().skip(state.help_offset).take(count).enumerate() {
            row(&mut output, i, line, width, false, color)?;
        }
        row(
            &mut output,
            height - 1,
            "↑↓/PgUp/PgDn Scroll · Esc/Enter Close",
            width,
            true,
            color,
        )?;
        queue!(output, cursor::Hide)?;
        output.flush()?;
        return Ok(());
    }
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
    if state.viewing
        && let Some((category, _, entry)) = results.get(state.selected)
    {
        return render_details(&mut output, category, entry, state, width, height, color);
    }
    let total: usize = data.values().map(Vec::len).sum();
    row(
        &mut output,
        0,
        &selection_title.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "cmdnavi   |   {} categories   ·   {total} commands",
                data.len()
            )
        }),
        width,
        true,
        color,
    )?;
    let mut search_start = 0;
    let available = width.saturating_sub(10);
    while state.query[search_start..state.cursor].width() > available {
        search_start += state.query[search_start..]
            .graphemes(true)
            .next()
            .map(str::len)
            .unwrap_or(1);
    }
    row(
        &mut output,
        2,
        &format!("Search > {}", &state.query[search_start..]),
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
                "No matches. Ctrl+W removes a word; Ctrl+U clears."
            },
            width,
            false,
            color,
        )?;
    }
    for (i, (category, _, entry)) in results.iter().enumerate().skip(start).take(list_height) {
        let marker = if i == state.selected { ">" } else { " " };
        result_row(
            &mut output,
            4 + i - start,
            &format!("{marker} [{category}] {}", entry.command),
            width,
            i == state.selected,
            color,
            &state.query,
        )?;
    }
    let detail_start = 5 + list_height;
    let detail_height = height.saturating_sub(detail_start + 4);
    if let Some((category, _, entry)) = results.get(state.selected) {
        let detail = wrap(&entry_details(entry), width);
        state.detail_offset = state
            .detail_offset
            .min(detail.len().saturating_sub(detail_height));
        row(
            &mut output,
            detail_start - 1,
            &format!(
                "── [{category}] · detail {}/{} · PgUp/PgDn scroll",
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
            result_row(
                &mut output,
                detail_start + i,
                line,
                width,
                false,
                color,
                &state.query,
            )?;
        }
    }
    // Reserve two rows for status, so clipboard setup guidance remains visible.
    for (i, line) in wrap(&state.message, width).iter().take(2).enumerate() {
        row(&mut output, height - 4 + i, line, width, false, color)?;
    }
    row(
        &mut output,
        height - 2,
        if selection_title.is_some() {
            "↑↓ Select · Enter Choose · Esc Cancel"
        } else if width < 58 {
            "↑↓ Select · Enter Details · F1 Help"
        } else {
            "↑↓ Select · Enter Details · Ctrl+A Add · Ctrl+E Edit · F1 Help"
        },
        width,
        true,
        color,
    )?;
    row(
        &mut output,
        height - 1,
        if selection_title.is_some() {
            "Type to filter · Ctrl+U Clear · F1 Help"
        } else if width < 58 {
            "Esc Quit · ^A Add · ^E Edit · F1 Help"
        } else {
            "Esc Quit · Ctrl+U Clear · Ctrl+D Remove · Ctrl+Z Undo · F1 Help"
        },
        width,
        false,
        color,
    )?;
    let cursor_x = (9 + state.query[search_start..state.cursor].width()).min(width);
    queue!(output, cursor::MoveTo(cursor_x as u16, 2), cursor::Show)?;
    output.flush()?;
    Ok(())
}

fn entry_details(entry: &CommandEntry) -> String {
    format!(
        "Command\n{}\n\nDescription\n{}",
        entry.command,
        if entry.description.is_empty() {
            "No description yet. Add one when editing."
        } else {
            &entry.description
        }
    )
}

fn render_details(
    output: &mut impl Write,
    category: &str,
    entry: &CommandEntry,
    state: &mut State,
    width: usize,
    height: usize,
    color: bool,
) -> Result<()> {
    row(
        output,
        0,
        &format!("Command details · [{category}]"),
        width,
        true,
        color,
    )?;
    let lines = wrap(&entry_details(entry), width);
    let visible = height.saturating_sub(7);
    state.detail_offset = state.detail_offset.min(lines.len().saturating_sub(visible));
    for (i, line) in lines
        .iter()
        .skip(state.detail_offset)
        .take(visible)
        .enumerate()
    {
        result_row(output, i + 2, line, width, false, color, &state.query)?;
    }
    row(output, height - 4, &state.message, width, false, color)?;
    row(
        output,
        height - 3,
        "c Copy · e Edit · d Remove",
        width,
        true,
        color,
    )?;
    row(
        output,
        height - 2,
        "m Move · n Rename · u Undo",
        width,
        false,
        color,
    )?;
    row(
        output,
        height - 1,
        "Esc Back · ↑↓ Scroll · ? Help",
        width,
        false,
        color,
    )?;
    queue!(output, cursor::Hide)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removing_a_word_keeps_the_suffix_and_a_valid_unicode_cursor() {
        let mut state = State {
            query: "障害 調査  suffix".into(),
            cursor: "障害 調査  ".len(),
            ..Default::default()
        };
        state.remove_word();
        assert_eq!(state.query, "障害 suffix");
        assert_eq!(state.cursor, "障害 ".len());
        state.remove_word();
        assert_eq!(state.query, "suffix");
        assert_eq!(state.cursor, 0);
        state.remove_word();
        assert_eq!(state.query, "suffix");
    }

    #[test]
    fn highlighted_rows_preserve_text_and_do_not_enable_color_with_no_color() {
        let mut output = Vec::new();
        result_row(&mut output, 0, "IP 日本語", 80, false, false, "ip 日本").unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\x1b[4mIP\x1b[24m"));
        assert!(output.contains("\x1b[4m日本\x1b[24m語"));
        assert!(!output.contains("38;"));
    }
    #[test]
    fn failed_reload_preserves_data_and_displays_the_error_instead_of_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut data = Config::from([(
            "notes".into(),
            vec![CommandEntry {
                command: "echo hello".into(),
                description: String::new(),
            }],
        )]);
        let original = data.clone();
        let mut state = State::default();
        std::fs::write(&path, "notes: [invalid").unwrap();
        for message in [None, Some("Reloaded."), Some("Last change undone.")] {
            assert!(!reload(&path, &mut data, &mut state, message));
            assert_eq!(data, original);
            assert!(state.help);
            assert!(state.message.starts_with("Reload failed."));
            assert!(state.message.contains("invalid YAML"));
        }
        config::save(&path, &original).unwrap();
        assert!(reload(&path, &mut data, &mut state, Some("Reloaded.")));
        assert_eq!(state.message, "Reloaded.");
    }
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
