use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Config = BTreeMap<String, Vec<CommandEntry>>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEntry {
    pub command: String,
    #[serde(default)]
    pub description: String,
}

impl CommandEntry {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.command.trim().is_empty() {
            return Err("command must not be empty");
        }
        if self
            .command
            .chars()
            .chain(self.description.chars())
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        {
            return Err("command and description must not contain terminal control characters");
        }
        Ok(())
    }
}

pub fn validate_config(config: &Config) -> Result<(), String> {
    for (category, entries) in config {
        validate_category(category)?;
        for entry in entries {
            entry
                .validate()
                .map_err(|e| format!("category \"{category}\": {e}"))?;
        }
    }
    Ok(())
}

pub fn validate_category(category: &str) -> Result<(), String> {
    const RESERVED: &[&str] = &["add", "edit", "list", "remove", "help", "version"];
    if category.is_empty()
        || !category
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "category \"{category}\" contains characters that cannot be used.\nUse letters (including Japanese), numbers, '_' or '-', such as network or 障害調査. Spaces and punctuation are not allowed."
        ));
    }
    if RESERVED.contains(&category) {
        return Err(format!(
            "category name \"{category}\" is reserved.\nPlease choose a different category name."
        ));
    }
    Ok(())
}

/// Byte ranges in the original string, including Unicode lowercase expansions.
pub fn match_ranges(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut source = Vec::new();
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        for lower in character.to_lowercase() {
            folded.push(lower);
            source.extend(std::iter::repeat_n(start..end, lower.len_utf8()));
        }
    }
    let mut ranges = Vec::new();
    for word in query.split_whitespace() {
        let word = word.to_lowercase();
        for (start, _) in folded.match_indices(&word) {
            ranges.push(source[start].start..source[start + word.len() - 1].end);
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

pub fn matches_query(category: &str, entry: &CommandEntry, query: &str) -> bool {
    let haystack = format!("{category}\n{}\n{}", entry.command, entry.description).to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_validation_rejects_invalid_and_reserved_names() {
        assert!(validate_category("service-op").is_ok());
        assert!(validate_category("bad name").is_err());
        assert!(validate_category("add").is_err());
        assert!(validate_category("障害調査_メモ-2").is_ok());
        assert!(validate_category("メモ\n別名").is_err());
    }

    #[test]
    fn match_ranges_preserve_unicode_boundaries_and_merge_overlaps() {
        let text = "İP 日本語 ip";
        let ranges = match_ranges(text, "i 日本 語 ip");
        let matched: Vec<_> = ranges.iter().map(|range| &text[range.clone()]).collect();
        assert_eq!(matched, ["İ", "日本語", "ip"]);
        let ranges = match_ranges("network", "net network");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 0..7);
        assert!(match_ranges(text, "   ").is_empty());
    }
}
