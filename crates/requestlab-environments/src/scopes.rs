//! Variable scopes: global, collection, environment and local values.
//!
//! Resolution order is Postman's: a local value wins over the environment, the
//! environment over the collection, and the collection over global values. A
//! disabled definition is skipped rather than shadowing the scope below it, so
//! turning a local value off falls back to the environment instead of failing.

use requestlab_models::{Environment, EnvironmentVariable};

use crate::errors::{ResolutionError, ResolutionFailure};
use crate::resolver::{resolve_text, Resolution};

/// Where a variable was defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariableScope {
    Global,
    Collection,
    Environment,
    Local,
}

impl VariableScope {
    pub const ALL: [VariableScope; 4] = [
        VariableScope::Global,
        VariableScope::Collection,
        VariableScope::Environment,
        VariableScope::Local,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VariableScope::Global => "Global",
            VariableScope::Collection => "Collection",
            VariableScope::Environment => "Environment",
            VariableScope::Local => "Local",
        }
    }
}

/// A resolved variable together with the scope that supplied it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedValue {
    pub scope: VariableScope,
    pub variable: EnvironmentVariable,
}

/// The variables visible to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableScopes {
    pub global: Vec<EnvironmentVariable>,
    pub collection: Vec<EnvironmentVariable>,
    pub environment: Environment,
    pub local: Vec<EnvironmentVariable>,
}

impl Default for VariableScopes {
    fn default() -> Self {
        Self {
            global: Vec::new(),
            collection: Vec::new(),
            environment: Environment::new("No environment"),
            local: Vec::new(),
        }
    }
}

impl VariableScopes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_environment(environment: Environment) -> Self {
        Self {
            environment,
            ..Self::default()
        }
    }

    /// The definition that wins for `key`, if any.
    pub fn get(&self, key: &str) -> Option<ScopedValue> {
        let key = key.trim();
        let scopes = [
            (VariableScope::Local, &self.local),
            (VariableScope::Environment, &self.environment.variables),
            (VariableScope::Collection, &self.collection),
            (VariableScope::Global, &self.global),
        ];
        for (scope, variables) in scopes {
            if let Some(variable) = variables
                .iter()
                .find(|variable| variable.enabled && variable.key.trim() == key)
            {
                return Some(ScopedValue {
                    scope,
                    variable: variable.clone(),
                });
            }
        }
        None
    }

    /// Every enabled variable name visible from any scope, highest precedence
    /// first and without duplicates. Used for autocomplete.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let scopes = [
            &self.local,
            &self.environment.variables,
            &self.collection,
            &self.global,
        ];
        for variables in scopes {
            for variable in variables.iter().filter(|variable| variable.enabled) {
                let key = variable.key.trim();
                if !key.is_empty() && !names.iter().any(|existing| existing == key) {
                    names.push(key.to_string());
                }
            }
        }
        names
    }

    /// Names starting with `prefix`, for the `{{` completion popup.
    pub fn matching(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim();
        self.names()
            .into_iter()
            .filter(|name| name.starts_with(prefix))
            .collect()
    }

    /// Flatten every visible definition into one environment for resolution.
    pub fn merged(&self) -> Environment {
        let mut merged = Environment::new(&self.environment.name);
        merged.id = self.environment.id;
        for scope in [
            &self.global,
            &self.collection,
            &self.environment.variables,
            &self.local,
        ] {
            for variable in scope.iter().filter(|variable| variable.enabled) {
                let key = variable.key.trim();
                if key.is_empty() {
                    continue;
                }
                merged
                    .variables
                    .retain(|existing| existing.key.trim() != key);
                merged.variables.push(variable.clone());
            }
        }
        merged
    }

    /// Resolve every `{{...}}` expression using the merged scopes.
    pub fn resolve_text(&self, input: &str) -> Result<Resolution, Vec<ResolutionError>> {
        resolve_text(input, &self.merged())
    }

    /// Resolve a whole request, reporting which variable names were used.
    pub fn resolve_request(
        &self,
        request: &requestlab_models::Request,
    ) -> Result<crate::resolver::ResolvedRequest, ResolutionFailure> {
        crate::resolver::resolve_request(request, &self.merged())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{HeaderEntry, HttpMethod, Request};

    fn scopes() -> VariableScopes {
        let mut environment = Environment::new("Staging");
        environment.variables.push(EnvironmentVariable::new(
            "BASE_URL",
            "https://staging.example.com",
        ));
        environment
            .variables
            .push(EnvironmentVariable::secret("TOKEN", "env-token"));

        VariableScopes {
            global: vec![
                EnvironmentVariable::new("BASE_URL", "https://global.example.com"),
                EnvironmentVariable::new("GLOBAL_ONLY", "g"),
            ],
            collection: vec![
                EnvironmentVariable::new("COLLECTION_ONLY", "c"),
                EnvironmentVariable::new("TOKEN", "collection-token"),
            ],
            environment,
            local: vec![EnvironmentVariable::new("LOCAL_ONLY", "l")],
        }
    }

    #[test]
    fn higher_scopes_win() {
        let scopes = scopes();
        assert_eq!(
            scopes.get("BASE_URL").unwrap().scope,
            VariableScope::Environment
        );
        assert_eq!(
            scopes.get("TOKEN").unwrap().variable.value,
            "env-token",
            "environment beats collection"
        );
        assert_eq!(
            scopes.get("GLOBAL_ONLY").unwrap().scope,
            VariableScope::Global
        );
        assert_eq!(
            scopes.get("COLLECTION_ONLY").unwrap().scope,
            VariableScope::Collection
        );
        assert_eq!(
            scopes.get("LOCAL_ONLY").unwrap().scope,
            VariableScope::Local
        );
        assert!(scopes.get("MISSING").is_none());
    }

    #[test]
    fn local_values_shadow_everything() {
        let mut scopes = scopes();
        scopes.local.push(EnvironmentVariable::new(
            "BASE_URL",
            "http://localhost:3000",
        ));
        assert_eq!(
            scopes.get("BASE_URL").unwrap().variable.value,
            "http://localhost:3000"
        );
        let resolution = scopes.resolve_text("{{BASE_URL}}/v1").unwrap();
        assert_eq!(resolution.value, "http://localhost:3000/v1");
    }

    #[test]
    fn disabled_definitions_fall_back_to_the_scope_below() {
        let mut scopes = scopes();
        let mut disabled = EnvironmentVariable::new("BASE_URL", "http://off");
        disabled.enabled = false;
        scopes.local.push(disabled);
        assert_eq!(
            scopes.get("BASE_URL").unwrap().scope,
            VariableScope::Environment
        );
    }

    #[test]
    fn resolution_covers_every_request_field_and_reports_names() {
        let mut request = Request::new(HttpMethod::Post, "{{BASE_URL}}/users");
        request
            .headers
            .push(HeaderEntry::new("Authorization", "Bearer {{TOKEN}}"));
        request
            .headers
            .push(HeaderEntry::new("X-Local", "{{LOCAL_ONLY}}"));
        let resolved = scopes().resolve_request(&request).unwrap();
        assert_eq!(resolved.request.url, "https://staging.example.com/users");
        assert_eq!(resolved.request.headers[0].value, "Bearer env-token");
        assert_eq!(resolved.request.headers[1].value, "l");
        assert!(resolved.variables.contains(&"BASE_URL".to_string()));
        assert!(resolved.variables.contains(&"TOKEN".to_string()));
    }

    #[test]
    fn names_are_deduplicated_and_ordered_by_precedence() {
        let names = scopes().names();
        assert_eq!(names[0], "LOCAL_ONLY");
        assert!(names.contains(&"BASE_URL".to_string()));
        assert_eq!(names.iter().filter(|name| *name == "BASE_URL").count(), 1);
        assert!(names.contains(&"TOKEN".to_string()));
        assert!(names.contains(&"GLOBAL_ONLY".to_string()));
    }

    #[test]
    fn matching_filters_by_prefix() {
        let scopes = scopes();
        assert_eq!(
            scopes.matching("LOCAL"),
            vec!["LOCAL_ONLY".to_string()],
            "the local scope is matched too"
        );
        assert!(scopes.matching("ZZZ").is_empty());
        assert!(scopes.matching("").len() >= 5);
    }

    #[test]
    fn undefined_variables_still_fail_loudly() {
        let errors = scopes().resolve_text("{{NOPE}}").unwrap_err();
        assert!(matches!(errors[0], ResolutionError::Undefined { .. }));
    }
}
