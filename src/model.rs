use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Config = BTreeMap<String, Vec<CommandEntry>>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEntry {
    pub command: String,
    pub description: String,
}

impl CommandEntry {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.command.trim().is_empty() {
            return Err("command must not be empty");
        }
        if self.description.trim().is_empty() {
            return Err("description must not be empty");
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
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "category \"{category}\" contains characters that cannot be used.\nTry a name made from letters, numbers, '_' or '-', such as network or service-op."
        ));
    }
    if RESERVED.contains(&category) {
        return Err(format!(
            "category name \"{category}\" is reserved.\nPlease choose a different category name."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_validation_rejects_invalid_and_reserved_names() {
        assert!(validate_category("service-op").is_ok());
        assert!(validate_category("bad name").is_err());
        assert!(validate_category("add").is_err());
    }
}
