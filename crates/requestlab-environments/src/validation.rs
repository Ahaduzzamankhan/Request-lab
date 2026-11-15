//! Validation and redaction for environment variables.

use requestlab_models::EnvironmentVariable;

use crate::resolver::is_valid_variable_name;

/// Validate a variable list before saving.
///
/// Returns one message per problem so the UI can show them next to the editor.
pub fn validate_variables(variables: &[EnvironmentVariable]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for (index, variable) in variables.iter().enumerate() {
        let key = variable.key.trim();
        if key.is_empty() {
            if !variable.value.is_empty() {
                problems.push(format!(
                    "Row {} has a value but no variable name",
                    index + 1
                ));
            }
            continue;
        }
        if !is_valid_variable_name(key) {
            problems.push(format!(
                "'{key}' is not a valid variable name; use letters, numbers, '_', '-' or '.'"
            ));
        }
        let lowered = key.to_ascii_lowercase();
        if seen.contains(&lowered) {
            problems.push(format!("'{key}' is defined more than once"));
        } else {
            seen.push(lowered);
        }
    }

    problems
}

/// Redact secret values for diagnostics: `{"API_TOKEN": "[redacted]"}`.
pub fn redacted_map(variables: &[EnvironmentVariable]) -> Vec<(String, String)> {
    variables
        .iter()
        .map(|variable| {
            let value = if variable.is_secret {
                requestlab_models::REDACTED.to_string()
            } else {
                variable.value.clone()
            };
            (variable.key.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_list() {
        let variables = vec![
            EnvironmentVariable::new("BASE_URL", "http://localhost:3000"),
            EnvironmentVariable::secret("API_TOKEN", "tok"),
        ];
        assert!(validate_variables(&variables).is_empty());
    }

    #[test]
    fn rejects_duplicates_and_bad_names() {
        let variables = vec![
            EnvironmentVariable::new("A", "1"),
            EnvironmentVariable::new("a", "2"),
            EnvironmentVariable::new("BAD NAME", "3"),
            EnvironmentVariable::new(" ", "orphan"),
        ];
        let problems = validate_variables(&variables);
        assert_eq!(problems.len(), 3);
        assert!(problems.iter().any(|problem| problem.contains("more than once")));
        assert!(problems.iter().any(|problem| problem.contains("not a valid variable name")));
        assert!(problems.iter().any(|problem| problem.contains("no variable name")));
    }

    #[test]
    fn empty_rows_are_ignored() {
        let variables = vec![EnvironmentVariable::new("", "")];
        assert!(validate_variables(&variables).is_empty());
    }

    #[test]
    fn secrets_are_redacted() {
        let variables = vec![
            EnvironmentVariable::new("BASE_URL", "http://x"),
            EnvironmentVariable::secret("TOKEN", "secret"),
        ];
        let map = redacted_map(&variables);
        assert_eq!(map[0].1, "http://x");
        assert_eq!(map[1].1, requestlab_models::REDACTED);
    }
}
