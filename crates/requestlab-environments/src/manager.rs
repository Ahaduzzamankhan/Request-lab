//! The environment service: CRUD, active selection and request resolution.

use requestlab_models::{now_rfc3339, Environment, EnvironmentVariable, Request};
use requestlab_storage::{
    repositories::{environments as repo, settings},
    Storage, ACTIVE_ENVIRONMENT_KEY,
};
use uuid::Uuid;

use crate::errors::EnvironmentError;
use crate::resolver::{resolve_request, ResolvedRequest};
use crate::validation::validate_variables;

/// Coordinates environment data and variable resolution.
#[derive(Debug, Clone)]
pub struct EnvironmentService {
    storage: Storage,
}

impl EnvironmentService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    fn pool(&self) -> &requestlab_storage::SqlitePool {
        self.storage.pool()
    }

    pub async fn list(&self) -> Result<Vec<Environment>, EnvironmentError> {
        Ok(repo::list(self.pool()).await?)
    }

    pub async fn get(&self, id: Uuid) -> Result<Environment, EnvironmentError> {
        Ok(repo::get(self.pool(), id).await?)
    }

    /// The selected environment id, if the setting still points at a real row.
    pub async fn active_id(&self) -> Result<Option<Uuid>, EnvironmentError> {
        let Some(raw) = settings::get(self.pool(), ACTIVE_ENVIRONMENT_KEY).await? else {
            return Ok(None);
        };
        let Ok(id) = Uuid::parse_str(&raw) else {
            return Ok(None);
        };
        match repo::get(self.pool(), id).await {
            Ok(_) => Ok(Some(id)),
            Err(_) => {
                // The environment was deleted while selected: clear the setting.
                settings::remove(self.pool(), ACTIVE_ENVIRONMENT_KEY).await?;
                Ok(None)
            }
        }
    }

    pub async fn active(&self) -> Result<Option<Environment>, EnvironmentError> {
        match self.active_id().await? {
            Some(id) => Ok(Some(repo::get(self.pool(), id).await?)),
            None => Ok(None),
        }
    }

    pub async fn set_active(&self, id: Option<Uuid>) -> Result<(), EnvironmentError> {
        match id {
            Some(id) => {
                // Fail loudly if the environment does not exist.
                repo::get(self.pool(), id).await?;
                settings::set(self.pool(), ACTIVE_ENVIRONMENT_KEY, &id.to_string()).await?;
            }
            None => settings::remove(self.pool(), ACTIVE_ENVIRONMENT_KEY).await?,
        }
        Ok(())
    }

    pub async fn create(&self, name: &str) -> Result<Environment, EnvironmentError> {
        let name = normalized_name(name)?;
        let environment = Environment::new(name);
        repo::insert(self.pool(), &environment).await?;
        Ok(environment)
    }

    pub async fn rename(&self, id: Uuid, name: &str) -> Result<(), EnvironmentError> {
        let name = normalized_name(name)?;
        repo::rename(self.pool(), id, &name, &now_rfc3339()).await?;
        Ok(())
    }

    /// Delete an environment; if it was active, clear the selection.
    pub async fn delete(&self, id: Uuid) -> Result<(), EnvironmentError> {
        repo::delete(self.pool(), id).await?;
        if self.active_id().await? == Some(id) {
            self.set_active(None).await?;
        }
        Ok(())
    }

    pub async fn duplicate(&self, id: Uuid, name: &str) -> Result<Environment, EnvironmentError> {
        let name = normalized_name(name)?;
        let original = repo::get(self.pool(), id).await?;
        let now = now_rfc3339();
        let copy = Environment {
            id: Uuid::new_v4(),
            name,
            variables: original
                .variables
                .iter()
                .map(EnvironmentVariable::duplicate)
                .collect(),
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert(self.pool(), &copy).await?;
        Ok(copy)
    }

    /// Replace an environment's variables, validating the list first.
    pub async fn save_variables(
        &self,
        id: Uuid,
        variables: &[EnvironmentVariable],
    ) -> Result<(), EnvironmentError> {
        let problems = validate_variables(variables);
        if !problems.is_empty() {
            return Err(EnvironmentError::Invalid(problems.join("; ")));
        }
        let cleaned: Vec<EnvironmentVariable> = variables
            .iter()
            .filter(|variable| !variable.key.trim().is_empty())
            .map(|variable| EnvironmentVariable {
                key: variable.key.trim().to_string(),
                ..variable.clone()
            })
            .collect();
        repo::replace_variables(self.pool(), id, &cleaned, &now_rfc3339()).await?;
        Ok(())
    }

    /// Resolve a request against the active environment.
    pub async fn resolve(
        &self,
        request: &Request,
    ) -> Result<ResolvedRequest, EnvironmentError> {
        let active = self.active().await?;
        self.resolve_with_active(request, active.as_ref())
    }

    /// Resolve against an explicit environment (or none at all).
    pub fn resolve_with_active(
        &self,
        request: &Request,
        environment: Option<&Environment>,
    ) -> Result<ResolvedRequest, EnvironmentError> {
        let empty = Environment::new("No environment");
        let environment = environment.unwrap_or(&empty);
        Ok(resolve_request(request, environment)?)
    }
}

fn normalized_name(name: &str) -> Result<String, EnvironmentError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(EnvironmentError::EmptyName);
    }
    if trimmed.chars().count() > 80 {
        return Err(EnvironmentError::Invalid(
            "Environment names are limited to 80 characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{HeaderEntry, HttpMethod, RequestBody};

    async fn service() -> EnvironmentService {
        let storage = Storage::open_in_memory().await.unwrap();
        EnvironmentService::new(storage)
    }

    #[tokio::test]
    async fn creates_and_renames_environments() {
        let service = service().await;
        let created = service.create("  Development ").await.unwrap();
        assert_eq!(created.name, "Development");
        service.rename(created.id, "Dev").await.unwrap();
        let list = service.list().await.unwrap();
        assert_eq!(list[0].name, "Dev");
        assert!(matches!(
            service.create("   ").await,
            Err(EnvironmentError::EmptyName)
        ));
    }

    #[tokio::test]
    async fn variables_persist_and_resolve() {
        let service = service().await;
        let environment = service.create("Dev").await.unwrap();
        service
            .save_variables(
                environment.id,
                &[
                    EnvironmentVariable::new("BASE_URL", "http://localhost:3000"),
                    EnvironmentVariable::secret("API_TOKEN", "tok_1"),
                ],
            )
            .await
            .unwrap();
        service.set_active(Some(environment.id)).await.unwrap();

        let mut request = Request::new(HttpMethod::Post, "{{BASE_URL}}/login");
        request.headers.push(HeaderEntry::new("X-Token", "{{API_TOKEN}}"));
        request.body = RequestBody::json(r#"{"token":"{{API_TOKEN}}"}"#);
        let resolved = service.resolve(&request).await.unwrap();
        assert_eq!(resolved.request.url, "http://localhost:3000/login");
        assert_eq!(resolved.request.headers[0].value, "tok_1");
    }

    #[tokio::test]
    async fn duplicate_copies_variables() {
        let service = service().await;
        let environment = service.create("Dev").await.unwrap();
        service
            .save_variables(
                environment.id,
                &[EnvironmentVariable::new("A", "1")],
            )
            .await
            .unwrap();
        let copy = service.duplicate(environment.id, "Staging").await.unwrap();
        let copy = service.get(copy.id).await.unwrap();
        assert_eq!(copy.variables.len(), 1);
        assert_ne!(copy.variables[0].id, environment.variables.first().map(|v| v.id).unwrap_or_default());
    }

    #[tokio::test]
    async fn deleting_the_active_environment_clears_selection() {
        let service = service().await;
        let environment = service.create("Dev").await.unwrap();
        service.set_active(Some(environment.id)).await.unwrap();
        assert_eq!(service.active_id().await.unwrap(), Some(environment.id));
        service.delete(environment.id).await.unwrap();
        assert_eq!(service.active_id().await.unwrap(), None);
        assert!(service.active().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_invalid_variable_lists() {
        let service = service().await;
        let environment = service.create("Dev").await.unwrap();
        let error = service
            .save_variables(
                environment.id,
                &[
                    EnvironmentVariable::new("A", "1"),
                    EnvironmentVariable::new("A", "2"),
                ],
            )
            .await;
        assert!(matches!(error, Err(EnvironmentError::Invalid(_))));
    }

    #[tokio::test]
    async fn missing_variables_are_reported() {
        let service = service().await;
        let request = Request::new(HttpMethod::Get, "{{BASE_URL}}/users");
        let error = service.resolve(&request).await.unwrap_err();
        let message = error.to_string();
        assert!(message.contains("BASE_URL"));
    }

    #[tokio::test]
    async fn switching_environments_changes_resolution() {
        let service = service().await;
        let dev = service.create("Dev").await.unwrap();
        let prod = service.create("Prod").await.unwrap();
        service
            .save_variables(dev.id, &[EnvironmentVariable::new("BASE_URL", "http://dev.local")])
            .await
            .unwrap();
        service
            .save_variables(prod.id, &[EnvironmentVariable::new("BASE_URL", "https://api.example.com")])
            .await
            .unwrap();
        let request = Request::new(HttpMethod::Get, "{{BASE_URL}}/health");

        service.set_active(Some(dev.id)).await.unwrap();
        assert_eq!(
            service.resolve(&request).await.unwrap().request.url,
            "http://dev.local/health"
        );
        service.set_active(Some(prod.id)).await.unwrap();
        assert_eq!(
            service.resolve(&request).await.unwrap().request.url,
            "https://api.example.com/health"
        );
    }
}
