//! Environment and variable models.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    /// Secret variables are hidden in the UI and redacted from diagnostics.
    pub is_secret: bool,
}

impl EnvironmentVariable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            value: value.into(),
            enabled: true,
            is_secret: false,
        }
    }

    pub fn secret(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            is_secret: true,
            ..Self::new(key, value)
        }
    }

    pub fn duplicate(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            ..self.clone()
        }
    }

    /// Value shown in the UI: masked when secret and not explicitly revealed.
    pub fn display_value(&self, reveal: bool) -> String {
        if self.is_secret && !reveal && !self.value.is_empty() {
            "•".repeat(self.value.chars().count().min(16))
        } else {
            self.value.clone()
        }
    }

    pub fn is_valid_key(&self) -> bool {
        !self.key.trim().is_empty()
    }
}

/// A named set of variables that can be selected as active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub variables: Vec<EnvironmentVariable>,
    pub created_at: String,
    pub updated_at: String,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        let now = crate::common::now_rfc3339();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            variables: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Look up an enabled variable by key (case-sensitive, trimmed).
    pub fn value_of(&self, key: &str) -> Option<&str> {
        let key = key.trim();
        self.variables
            .iter()
            .find(|variable| variable.enabled && variable.key.trim() == key)
            .map(|variable| variable.value.as_str())
    }

    /// Variable names that are enabled and usable for resolution.
    pub fn active_keys(&self) -> Vec<&str> {
        self.variables
            .iter()
            .filter(|variable| variable.enabled && variable.is_valid_key())
            .map(|variable| variable.key.trim())
            .collect()
    }

    pub fn has_key(&self, key: &str) -> bool {
        let key = key.trim();
        self.variables
            .iter()
            .any(|variable| variable.key.trim() == key)
    }

    pub fn find_variable(&self, key: &str) -> Option<&EnvironmentVariable> {
        let key = key.trim();
        self.variables
            .iter()
            .find(|variable| variable.key.trim() == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_only_enabled_variables() {
        let mut environment = Environment::new("Development");
        environment
            .variables
            .push(EnvironmentVariable::new("BASE_URL", "http://localhost:3000"));
        let mut disabled = EnvironmentVariable::new("API_VERSION", "v1");
        disabled.enabled = false;
        environment.variables.push(disabled);

        assert_eq!(
            environment.value_of("BASE_URL"),
            Some("http://localhost:3000")
        );
        assert_eq!(environment.value_of("API_VERSION"), None);
        assert_eq!(environment.value_of("MISSING"), None);
        assert_eq!(environment.active_keys(), vec!["BASE_URL"]);
        assert!(environment.has_key("API_VERSION"));
    }

    #[test]
    fn secret_values_are_masked() {
        let variable = EnvironmentVariable::secret("API_TOKEN", "tok_123456");
        assert_eq!(variable.display_value(false), "•".repeat(10));
        assert_eq!(variable.display_value(true), "tok_123456");
        assert!(variable.is_secret);
    }

    #[test]
    fn duplicate_gets_new_identity() {
        let variable = EnvironmentVariable::new("A", "1");
        let copy = variable.duplicate();
        assert_ne!(copy.id, variable.id);
        assert_eq!(copy.key, "A");
    }

    #[test]
    fn blank_keys_are_invalid() {
        assert!(!EnvironmentVariable::new("  ", "x").is_valid_key());
        assert!(EnvironmentVariable::new("A", "x").is_valid_key());
    }
}
