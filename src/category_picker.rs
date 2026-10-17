//! Searchable category selection; the YAML is changed only after the form is saved.
use crate::Result;
use crate::browser::{Screen, clip, row};
use crate::model::{Config, validate_category};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    terminal::{self, Clear, ClearType},
};
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
struct Picker {
    query: String,
    cursor: usize,
    selected: usize,
}
impl Picker {
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
    fn insert(&mut self, text: &str) {
        self.query.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.selected = 0;
    }
    fn choices(&self, data: &Config, allow_new: bool) -> Vec<(String, bool)> {
        let query = self.query.trim().to_lowercase();
        if let Some(name) = query
            .strip_prefix('#')
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| data.keys().nth(i))
        {
            return vec![(name.clone(), false)];
        }
        let mut choices: Vec<_> = data
            .keys()
            .filter(|name| {
                query
                    .split_whitespace()
                    .all(|word| name.to_lowercase().contains(word))
            })
            .map(|name| (name.clone(), false))
            .collect();
        let new = self.new_name();
        if allow_new && validate_category(new).is_ok() && !data.contains_key(new) {
            choices.push((new.into(), true));
        }
        choices
    }
    fn new_name(&self) -> &str {
        if self.query.trim().is_empty() {
            "general"
        } else {
            self.query.trim()
        }
    }
}

pub(crate) fn choose(data: &Config, allow_new: bool, title: &str) -> Result<Option<String>> {
    let _screen = Screen::enter()?;
    let mut state = Picker::default();
    loop {
        let choices = state.choices(data, allow_new);
        state.selected = state.selected.min(choices.len().saturating_sub(1));
        render(data, &choices, &state, allow_new, title)?;
        let (width, height) = terminal::size()?;
        let usable = width >= 36 && height >= 10;
        match event::read()? {
            Event::Paste(text) if usable => state.insert(
                &text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>(),
            ),
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                if !usable && key.code != KeyCode::Esc && !(ctrl && key.code == KeyCode::Char('c'))
                {
                    continue;
                }
                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char('c') if ctrl => return Ok(None),
                    KeyCode::Char('u') if ctrl => state = Picker::default(),
                    KeyCode::Char('n')
                        if ctrl && allow_new && validate_category(state.new_name()).is_ok() =>
                    {
                        return Ok(Some(state.new_name().into()));
                    }
                    KeyCode::Enter | KeyCode::Char('j' | 'm')
                        if key.code == KeyCode::Enter || ctrl =>
                    {
                        if state.query.trim() == "/cancel" {
                            return Ok(None);
                        }
                        if let Some((name, _)) = choices.get(state.selected) {
                            return Ok(Some(name.clone()));
                        }
                    }
                    KeyCode::Tab => {
                        if let Some((name, _)) = choices.get(state.selected) {
                            state.query = name.clone();
                            state.cursor = state.query.len();
                            state.selected = 0;
                        }
                    }
                    KeyCode::Up => state.selected = state.selected.saturating_sub(1),
                    KeyCode::Down => {
                        state.selected = (state.selected + 1).min(choices.len().saturating_sub(1))
                    }
                    KeyCode::PageUp => state.selected = state.selected.saturating_sub(8),
                    KeyCode::PageDown => {
                        state.selected = (state.selected + 8).min(choices.len().saturating_sub(1))
                    }
                    KeyCode::Left => state.cursor = state.previous(),
                    KeyCode::Right => state.cursor = state.next(),
                    KeyCode::Home => state.cursor = 0,
                    KeyCode::End => state.cursor = state.query.len(),
                    KeyCode::Char('a') if ctrl => state.cursor = 0,
                    KeyCode::Char('e') if ctrl => state.cursor = state.query.len(),
                    KeyCode::Backspace => {
                        let previous = state.previous();
                        state.query.replace_range(previous..state.cursor, "");
                        state.cursor = previous;
                        state.selected = 0;
                    }
                    KeyCode::Delete => {
                        state.query.replace_range(state.cursor..state.next(), "");
                        state.selected = 0;
                    }
                    KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                        state.insert(&c.to_string())
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn render(
    data: &Config,
    choices: &[(String, bool)],
    state: &Picker,
    allow_new: bool,
    title: &str,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = usize::from(width).saturating_sub(1);
    let height = usize::from(height);
    let color = std::env::var_os("NO_COLOR").is_none();
    let mut output = io::stdout().lock();
    queue!(output, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
    if width < 35 || height < 10 {
        row(
            &mut output,
            0,
            "Enlarge terminal (36 x 10). Esc cancels.",
            width,
            false,
            color,
        )?;
        output.flush()?;
        return Ok(());
    }
    row(&mut output, 0, title, width, true, color)?;
    let label = "Category (name / filter): ";
    let available = width.saturating_sub(label.len() + 1);
    let mut start = 0;
    while state.query[start..state.cursor].width() > available {
        start += state.query[start..]
            .graphemes(true)
            .next()
            .map(str::len)
            .unwrap_or(1);
    }
    row(
        &mut output,
        2,
        &format!("{label}{}", &state.query[start..]),
        width,
        false,
        color,
    )?;
    let count = choices.iter().filter(|(_, new)| !new).count();
    row(
        &mut output,
        3,
        &format!("{count} matches · ↑↓ select · Enter choose · Tab fill"),
        width,
        false,
        color,
    )?;
    let visible = height.saturating_sub(7);
    let first = state.selected.saturating_sub(visible.saturating_sub(1));
    for (i, (name, new)) in choices.iter().enumerate().skip(first).take(visible) {
        let text = if *new {
            format!("Create [{name}]")
        } else {
            format!("[{name}] ({} commands)", data[name].len())
        };
        row(
            &mut output,
            4 + i - first,
            &format!("{} {text}", if i == state.selected { ">" } else { " " }),
            width,
            i == state.selected,
            color,
        )?;
    }
    if choices.is_empty() {
        let message = validate_category(state.new_name())
            .err()
            .unwrap_or_else(|| "No matching categories.".into());
        row(&mut output, 4, &clip(&message, width), width, false, color)?;
    }
    row(
        &mut output,
        height - 2,
        if !allow_new {
            "↑↓ Select · Enter Use · Esc Cancel"
        } else if width < 58 {
            "↑↓ Enter · ^N New · Esc Cancel"
        } else {
            "Ctrl+N use typed name · Ctrl+U clear · Esc cancel"
        },
        width,
        true,
        color,
    )?;
    let x = label.len() + state.query[start..state.cursor].width();
    queue!(output, cursor::MoveTo(x.min(width) as u16, 2), cursor::Show)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_uses_substrings_and_keeps_new_names_explicit() {
        let data = Config::from([("network".into(), vec![]), ("障害調査".into(), vec![])]);
        let mut picker = Picker {
            query: "WORK".into(),
            ..Default::default()
        };
        assert_eq!(
            picker.choices(&data, true),
            [("network".into(), false), ("WORK".into(), true)]
        );
        picker.query = "調査".into();
        assert_eq!(picker.choices(&data, true)[0], ("障害調査".into(), false));
        picker.query = "障害調査".into();
        assert_eq!(picker.choices(&data, true), [("障害調査".into(), false)]);
        picker.query = "bad name".into();
        assert!(picker.choices(&data, true).is_empty());
        picker.query = "new-category".into();
        assert!(picker.choices(&data, false).is_empty());
        assert_eq!(picker.choices(&data, true), [("new-category".into(), true)]);
    }
}
