//! `{{VARIABLE}}` interpolation.
//!
//! Rules:
//! - Only enabled variables resolve; a disabled or missing variable is an error,
//!   never a silent empty string.
//! - Substituted values are not scanned again, so a value containing `{{...}}`
//!   cannot cause recursive interpolation.
//! - `\{{` escapes a literal `{{` and the backslash is dropped.
//! - The caller's request is never modified; a resolved copy is returned.

use requestlab_models::{
    Authentication, Environment, Request, RequestBody, RequestField,
};

use crate::errors::{FieldError, ResolutionError, ResolutionFailure};

/// The outcome of resolving one string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolution {
    pub value: String,
    /// Variable names substituted, in first-use order.
    pub variables: Vec<String>,
}

/// A request with every template expression replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequest {
    pub request: Request,
    pub variables: Vec<String>,
}

/// Whether a variable name is syntactically valid.
pub fn is_valid_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

/// Resolve every `{{...}}` expression in `input`.
pub fn resolve_text(
    input: &str,
    environment: &Environment,
) -> Result<Resolution, Vec<ResolutionError>> {
    let mut out = String::with_capacity(input.len());
    let mut used: Vec<String> = Vec::new();
    let mut errors: Vec<ResolutionError> = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        let prefix = &rest[..start];
        if let Some(stripped) = prefix.strip_suffix('\\') {
            // Escaped opening braces: emit them literally and keep scanning after.
            out.push_str(stripped);
            out.push_str("{{");
            rest = &rest[start + 2..];
            continue;
        }
        out.push_str(prefix);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            errors.push(ResolutionError::Malformed(
                "missing closing '}}'".to_string(),
            ));
            rest = "";
            break;
        };
        let raw_name = &after[..end];
        let name = raw_name.trim();
        if name.is_empty() {
            errors.push(ResolutionError::Malformed(
                "empty variable name".to_string(),
            ));
        } else if !is_valid_variable_name(name) {
            errors.push(ResolutionError::Malformed(format!(
                "'{name}' is not a valid variable name"
            )));
        } else {
            match environment.find_variable(name) {
                None => errors.push(ResolutionError::Undefined {
                    name: name.to_string(),
                }),
                Some(variable) if !variable.enabled => errors.push(ResolutionError::Disabled {
                    name: name.to_string(),
                }),
                Some(variable) if variable.key.trim().is_empty() => {
                    errors.push(ResolutionError::Malformed(format!(
                        "'{name}' is not a valid variable name"
                    )))
                }
                Some(variable) => {
                    out.push_str(&variable.value);
                    if !used.iter().any(|existing| existing == name) {
                        used.push(name.to_string());
                    }
                }
            }
        }
        rest = &after[end + 2..];
    }

    out.push_str(rest);

    if errors.is_empty() {
        Ok(Resolution {
            value: out,
            variables: used,
        })
    } else {
        Err(errors)
    }
}

/// Resolve every field of a request against `environment`.
pub fn resolve_request(
    request: &Request,
    environment: &Environment,
) -> Result<ResolvedRequest, ResolutionFailure> {
    let mut problems: Vec<FieldError> = Vec::new();
    let mut variables: Vec<String> = Vec::new();
    let mut resolved = request.clone();

    let mut push = |field: RequestField,
                    result: Result<Resolution, Vec<ResolutionError>>,
                    problems: &mut Vec<FieldError>,
                    variables: &mut Vec<String>|
     -> Option<String> {
        match result {
            Ok(resolution) => {
                for name in resolution.variables {
                    if !variables.iter().any(|existing| existing == &name) {
                        variables.push(name);
                    }
                }
                Some(resolution.value)
            }
            Err(errors) => {
                problems.extend(errors.into_iter().map(|error| FieldError { field, error }));
                None
            }
        }
    };

    if let Some(value) = push(
        RequestField::Url,
        resolve_text(&request.url, environment),
        &mut problems,
        &mut variables,
    ) {
        resolved.url = value;
    }

    for index in 0..request.query_params.len() {
        let param = &request.query_params[index];
        if !param.enabled {
            continue;
        }
        if let Some(value) = push(
            RequestField::QueryParameter,
            resolve_text(&param.value, environment),
            &mut problems,
            &mut variables,
        ) {
            resolved.query_params[index].value = value;
        }
        if let Some(key) = push(
            RequestField::QueryParameter,
            resolve_text(&param.key, environment),
            &mut problems,
            &mut variables,
        ) {
            resolved.query_params[index].key = key;
        }
    }

    for index in 0..request.headers.len() {
        let header = &request.headers[index];
        if !header.enabled {
            continue;
        }
        if let Some(value) = push(
            RequestField::Header,
            resolve_text(&header.value, environment),
            &mut problems,
            &mut variables,
        ) {
            resolved.headers[index].value = value;
        }
        if let Some(key) = push(
            RequestField::Header,
            resolve_text(&header.key, environment),
            &mut problems,
            &mut variables,
        ) {
            resolved.headers[index].key = key;
        }
    }

    match &request.body {
        RequestBody::None => {}
        RequestBody::Json { content } => {
            if let Some(value) = push(
                RequestField::Body,
                resolve_text(content, environment),
                &mut problems,
                &mut variables,
            ) {
                resolved.body = RequestBody::Json { content: value };
            }
        }
        RequestBody::Raw {
            content,
            content_type,
        } => {
            if let Some(value) = push(
                RequestField::Body,
                resolve_text(content, environment),
                &mut problems,
                &mut variables,
            ) {
                resolved.body = RequestBody::Raw {
                    content: value,
                    content_type: content_type.clone(),
                };
            }
        }
        RequestBody::FormUrlEncoded { fields } => {
            let mut resolved_fields = fields.clone();
            for index in 0..fields.len() {
                if let Some(value) = push(
                    RequestField::Body,
                    resolve_text(&fields[index].value, environment),
                    &mut problems,
                    &mut variables,
                ) {
                    resolved_fields[index].value = value;
                }
            }
            resolved.body = RequestBody::FormUrlEncoded {
                fields: resolved_fields,
            };
        }
    }

    resolved.authentication = match &request.authentication {
        Authentication::Basic { username, password } => Authentication::Basic {
            username: push(
                RequestField::Authentication,
                resolve_text(username, environment),
                &mut problems,
                &mut variables,
            )
            .unwrap_or_else(|| username.clone()),
            password: push(
                RequestField::Authentication,
                resolve_text(password, environment),
                &mut problems,
                &mut variables,
            )
            .unwrap_or_else(|| password.clone()),
        },
        Authentication::Bearer { token } => Authentication::Bearer {
            token: push(
                RequestField::Authentication,
                resolve_text(token, environment),
                &mut problems,
                &mut variables,
            )
            .unwrap_or_else(|| token.clone()),
        },
        Authentication::ApiKey {
            key,
            value,
            location,
        } => Authentication::ApiKey {
            key: push(
                RequestField::Authentication,
                resolve_text(key, environment),
                &mut problems,
                &mut variables,
            )
            .unwrap_or_else(|| key.clone()),
            value: push(
                RequestField::Authentication,
                resolve_text(value, environment),
                &mut problems,
                &mut variables,
            )
            .unwrap_or_else(|| value.clone()),
            location: *location,
        },
        Authentication::None => Authentication::None,
    };

    if problems.is_empty() {
        resolved.name = request.name.clone();
        Ok(ResolvedRequest {
            request: resolved,
            variables,
        })
    } else {
        Err(ResolutionFailure::new(problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{EnvironmentVariable, HeaderEntry, HttpMethod, QueryParameter};

    fn environment() -> Environment {
        let mut environment = Environment::new("Development");
        environment.variables.push(EnvironmentVariable::new(
            "BASE_URL",
            "http://localhost:3000",
        ));
        environment
            .variables
            .push(EnvironmentVariable::secret("API_TOKEN", "tok_123"));
        environment
            .variables
            .push(EnvironmentVariable::new("API_VERSION", "v1"));
        let mut disabled = EnvironmentVariable::new("OFF", "nope");
        disabled.enabled = false;
        environment.variables.push(disabled);
        environment
    }

    #[test]
    fn resolves_multiple_variables() {
        let resolution =
            resolve_text("{{BASE_URL}}/{{API_VERSION}}/users", &environment()).unwrap();
        assert_eq!(resolution.value, "http://localhost:3000/v1/users");
        assert_eq!(resolution.variables, vec!["BASE_URL", "API_VERSION"]);
    }

    #[test]
    fn reports_undefined_variables() {
        let errors = resolve_text("{{MISSING}}/x", &environment()).unwrap_err();
        assert_eq!(
            errors,
            vec![ResolutionError::Undefined {
                name: "MISSING".into()
            }]
        );
    }

    #[test]
    fn reports_disabled_variables() {
        let errors = resolve_text("{{OFF}}", &environment()).unwrap_err();
        assert_eq!(
            errors,
            vec![ResolutionError::Disabled { name: "OFF".into() }]
        );
    }

    #[test]
    fn reports_malformed_expressions() {
        assert!(matches!(
            resolve_text("{{BASE_URL", &environment()).unwrap_err()[0],
            ResolutionError::Malformed(_)
        ));
        assert!(matches!(
            resolve_text("{{}}", &environment()).unwrap_err()[0],
            ResolutionError::Malformed(_)
        ));
        assert!(matches!(
            resolve_text("{{BAD NAME}}", &environment()).unwrap_err()[0],
            ResolutionError::Malformed(_)
        ));
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(
            resolve_text("plain text {braces}", &environment())
                .unwrap()
                .value,
            "plain text {braces}"
        );
    }

    #[test]
    fn escaping_produces_literal_braces() {
        let resolution = resolve_text(r"\{{NOT_A_VAR}}", &environment()).unwrap();
        assert_eq!(resolution.value, "{{NOT_A_VAR}}");
        assert!(resolution.variables.is_empty());
    }

    #[test]
    fn substitution_is_not_recursive() {
        let mut environment = Environment::new("Nested");
        environment
            .variables
            .push(EnvironmentVariable::new("A", "{{B}}"));
        environment.variables.push(EnvironmentVariable::new("B", "value"));
        let resolution = resolve_text("{{A}}", &environment).unwrap();
        assert_eq!(resolution.value, "{{B}}");
    }

    #[test]
    fn resolves_every_request_field() {
        let mut request = Request::new(HttpMethod::Post, "{{BASE_URL}}/{{API_VERSION}}/items");
        request
            .query_params
            .push(QueryParameter::new("token", "{{API_TOKEN}}"));
        request
            .headers
            .push(HeaderEntry::new("X-Version", "{{API_VERSION}}"));
        request
            .headers
            .push(HeaderEntry::new("X-Off", "{{OFF}}"));
        // A disabled header is skipped, so its placeholder is not an error.
        request.headers[1].enabled = false;
        request.body = RequestBody::json(r#"{"version":"{{API_VERSION}}"}"#);
        request.authentication = Authentication::Bearer {
            token: "{{API_TOKEN}}".into(),
        };

        let resolved = resolve_request(&request, &environment()).unwrap();
        assert_eq!(
            resolved.request.url,
            "http://localhost:3000/v1/items"
        );
        assert_eq!(resolved.request.query_params[0].value, "tok_123");
        assert_eq!(resolved.request.headers[0].value, "v1");
        assert_eq!(
            resolved.request.body.text(),
            Some(r#"{"version":"v1"}"#)
        );
        assert_eq!(
            resolved.request.authentication,
            Authentication::Bearer {
                token: "tok_123".into()
            }
        );
        // The original request stays untouched.
        assert_eq!(request.url, "{{BASE_URL}}/{{API_VERSION}}/items");
        assert_eq!(resolved.variables.len(), 3);
    }

    #[test]
    fn collects_problems_per_field() {
        let mut request = Request::new(HttpMethod::Get, "{{NOPE_URL}}");
        request.headers.push(HeaderEntry::new("X", "{{NOPE_HEADER}}"));
        let failure = resolve_request(&request, &environment()).unwrap_err();
        assert_eq!(failure.problems.len(), 2);
        assert_eq!(
            failure.missing_variables(),
            vec!["NOPE_URL".to_string(), "NOPE_HEADER".to_string()]
        );
        assert!(failure.problems[0].to_string().contains("NOPE_URL"));
    }

    #[test]
    fn no_active_environment_means_undefined() {
        let empty = Environment::new("Empty");
        let request = Request::new(HttpMethod::Get, "{{BASE_URL}}/x");
        let failure = resolve_request(&request, &empty).unwrap_err();
        assert_eq!(failure.missing_variables(), vec!["BASE_URL".to_string()]);
    }
}
