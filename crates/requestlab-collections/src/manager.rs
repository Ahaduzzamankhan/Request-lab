//! The collection service: one place that coordinates collection data.

use requestlab_models::{
    build_tree, folder_depth, now_rfc3339, Collection, CollectionTree, Folder, Request,
    SavedRequest, MAX_FOLDER_DEPTH,
};
use requestlab_storage::{repositories::collections as repo, Storage};
use uuid::Uuid;

use crate::errors::{normalized_name, CollectionError};
use crate::folders::{collect_folders, subtree_height};

/// Coordinates collection, folder and saved-request operations.
#[derive(Debug, Clone)]
pub struct CollectionService {
    storage: Storage,
}

impl CollectionService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    fn pool(&self) -> &requestlab_storage::SqlitePool {
        self.storage.pool()
    }

    /// Every collection with its folder/request tree, ready for the sidebar.
    pub async fn list_trees(&self) -> Result<Vec<CollectionTree>, CollectionError> {
        let collections = repo::list_collections(self.pool()).await?;
        let mut trees = Vec::with_capacity(collections.len());
        for collection in collections {
            trees.push(self.load_tree(collection).await?);
        }
        Ok(trees)
    }

    pub async fn tree(&self, collection_id: Uuid) -> Result<CollectionTree, CollectionError> {
        let collection = repo::get_collection(self.pool(), collection_id).await?;
        self.load_tree(collection).await
    }

    async fn load_tree(&self, collection: Collection) -> Result<CollectionTree, CollectionError> {
        let folders = repo::list_folders(self.pool(), collection.id).await?;
        let requests = repo::list_saved_requests(self.pool(), collection.id).await?;
        Ok(build_tree(collection, folders, requests))
    }

    // ------------------------------------------------------------ collections

    pub async fn create_collection(&self, name: &str) -> Result<Collection, CollectionError> {
        let name = normalized_name(name)?;
        let now = now_rfc3339();
        let collection = Collection {
            id: Uuid::new_v4(),
            name,
            description: None,
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert_collection(self.pool(), &collection).await?;
        tracing::info!(%collection.id, "collection created");
        Ok(collection)
    }

    pub async fn rename_collection(
        &self,
        collection_id: Uuid,
        name: &str,
    ) -> Result<(), CollectionError> {
        let name = normalized_name(name)?;
        repo::rename_collection(self.pool(), collection_id, &name, &now_rfc3339()).await?;
        Ok(())
    }

    pub async fn delete_collection(&self, collection_id: Uuid) -> Result<(), CollectionError> {
        repo::delete_collection(self.pool(), collection_id).await?;
        tracing::info!(%collection_id, "collection deleted");
        Ok(())
    }

    /// Copy a collection, its folders and its requests under a new name.
    pub async fn duplicate_collection(
        &self,
        collection_id: Uuid,
        name: &str,
    ) -> Result<Collection, CollectionError> {
        let name = normalized_name(name)?;
        let original = self.tree(collection_id).await?;
        let now = now_rfc3339();
        let copy = Collection {
            id: Uuid::new_v4(),
            name,
            description: original.collection.description.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        repo::insert_collection(self.pool(), &copy).await?;

        let mut folder_map: Vec<(Uuid, Uuid)> = Vec::new();
        for folder in collect_folders(&original.root) {
            let new_id = Uuid::new_v4();
            folder_map.push((folder.id, new_id));
            let parent_id = folder.parent_id.map(|parent| {
                folder_map
                    .iter()
                    .find(|(old, _)| *old == parent)
                    .map(|(_, new)| *new)
                    .unwrap_or(parent)
            });
            let new_folder = Folder {
                id: new_id,
                collection_id: copy.id,
                parent_id,
                name: folder.name.clone(),
                sort_order: folder.sort_order,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            repo::insert_folder(self.pool(), &new_folder).await?;
        }

        for saved in original.all_requests() {
            let folder_id = saved
                .folder_id
                .and_then(|old| folder_map.iter().find(|(o, _)| *o == old).map(|(_, n)| *n));
            let duplicate = SavedRequest {
                id: Uuid::new_v4(),
                collection_id: copy.id,
                folder_id,
                name: saved.name.clone(),
                request: saved.request.duplicate_request(),
                sort_order: saved.sort_order,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            repo::insert_saved_request(self.pool(), &duplicate).await?;
        }

        Ok(copy)
    }

    // ---------------------------------------------------------------- folders

    pub async fn create_folder(
        &self,
        collection_id: Uuid,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Folder, CollectionError> {
        let name = normalized_name(name)?;
        let tree = self.tree(collection_id).await?;

        if let Some(parent_id) = parent_id {
            if tree.folder(parent_id).is_none() {
                return Err(CollectionError::Storage(
                    requestlab_storage::StorageError::NotFound { entity: "Folder" },
                ));
            }
            let folders: Vec<Folder> = collect_folders(&tree.root).into_iter().cloned().collect();
            if folder_depth(&folders, parent_id) >= MAX_FOLDER_DEPTH {
                return Err(CollectionError::TooDeep);
            }
        }

        let sort_order = repo::next_folder_order(self.pool(), collection_id, parent_id).await?;
        let now = now_rfc3339();
        let folder = Folder {
            id: Uuid::new_v4(),
            collection_id,
            parent_id,
            name,
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert_folder(self.pool(), &folder).await?;
        Ok(folder)
    }

    pub async fn rename_folder(&self, folder_id: Uuid, name: &str) -> Result<(), CollectionError> {
        let name = normalized_name(name)?;
        repo::rename_folder(self.pool(), folder_id, &name, &now_rfc3339()).await?;
        Ok(())
    }

    pub async fn delete_folder(&self, folder_id: Uuid) -> Result<(), CollectionError> {
        repo::delete_folder(self.pool(), folder_id).await?;
        Ok(())
    }

    /// Move a folder under a new parent, rejecting cycles and depth overflow.
    pub async fn move_folder(
        &self,
        folder_id: Uuid,
        new_parent: Option<Uuid>,
    ) -> Result<(), CollectionError> {
        let folder = repo::get_folder(self.pool(), folder_id).await?;
        let tree = self.tree(folder.collection_id).await?;

        if let Some(new_parent) = new_parent {
            if new_parent == folder_id {
                return Err(CollectionError::InvalidMove);
            }
            if tree.folder_is_descendant(new_parent, folder_id) {
                return Err(CollectionError::InvalidMove);
            }
            let folders: Vec<Folder> = collect_folders(&tree.root).into_iter().cloned().collect();
            if folder_depth(&folders, new_parent) + subtree_height(&tree, folder_id)
                > MAX_FOLDER_DEPTH
            {
                return Err(CollectionError::TooDeep);
            }
        }

        let order = repo::next_folder_order(self.pool(), folder.collection_id, new_parent).await?;
        repo::move_folder(self.pool(), folder_id, new_parent, &now_rfc3339()).await?;
        repo::set_folder_order(self.pool(), folder_id, order).await?;
        Ok(())
    }

    // --------------------------------------------------------------- requests

    pub async fn save_request(
        &self,
        collection_id: Uuid,
        folder_id: Option<Uuid>,
        name: &str,
        request: &Request,
    ) -> Result<SavedRequest, CollectionError> {
        let name = normalized_name(name)?;
        if let Some(folder_id) = folder_id {
            let folder = repo::get_folder(self.pool(), folder_id).await?;
            if folder.collection_id != collection_id {
                return Err(CollectionError::Invalid(
                    "The destination folder belongs to another collection".to_string(),
                ));
            }
        }
        let sort_order = repo::next_request_order(self.pool(), collection_id, folder_id).await?;
        let now = now_rfc3339();
        let mut stored_request = request.clone();
        stored_request.id = Uuid::new_v4();
        stored_request.name = name.clone();
        let saved = SavedRequest {
            id: Uuid::new_v4(),
            collection_id,
            folder_id,
            name,
            request: stored_request,
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert_saved_request(self.pool(), &saved).await?;
        Ok(saved)
    }

    pub async fn update_saved_request(
        &self,
        saved_id: Uuid,
        name: &str,
        request: &Request,
    ) -> Result<(), CollectionError> {
        let name = normalized_name(name)?;
        let existing = self.saved_request(saved_id).await?;
        let mut request = request.clone();
        request.name = name.clone();
        repo::update_saved_request(
            self.pool(),
            saved_id,
            &name,
            &request,
            existing.folder_id,
            &now_rfc3339(),
        )
        .await?;
        Ok(())
    }

    pub async fn saved_request(&self, saved_id: Uuid) -> Result<SavedRequest, CollectionError> {
        Ok(repo::get_saved_request(self.pool(), saved_id).await?)
    }

    pub async fn delete_saved_request(&self, saved_id: Uuid) -> Result<(), CollectionError> {
        repo::delete_saved_request(self.pool(), saved_id).await?;
        Ok(())
    }

    /// Duplicate a saved request next to itself.
    pub async fn duplicate_saved_request(
        &self,
        saved_id: Uuid,
    ) -> Result<SavedRequest, CollectionError> {
        let original = self.saved_request(saved_id).await?;
        let sort_order =
            repo::next_request_order(self.pool(), original.collection_id, original.folder_id)
                .await?;
        let now = now_rfc3339();
        let copy = SavedRequest {
            id: Uuid::new_v4(),
            collection_id: original.collection_id,
            folder_id: original.folder_id,
            name: format!("{} (copy)", original.display_name()),
            request: original.request.duplicate_request(),
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        };
        repo::insert_saved_request(self.pool(), &copy).await?;
        Ok(copy)
    }

    /// Move a request into another folder of the same collection.
    pub async fn move_saved_request(
        &self,
        saved_id: Uuid,
        target_folder: Option<Uuid>,
    ) -> Result<(), CollectionError> {
        let saved = self.saved_request(saved_id).await?;
        if let Some(folder_id) = target_folder {
            let folder = repo::get_folder(self.pool(), folder_id).await?;
            if folder.collection_id != saved.collection_id {
                return Err(CollectionError::Invalid(
                    "Requests can only move inside their own collection".to_string(),
                ));
            }
        }
        let order =
            repo::next_request_order(self.pool(), saved.collection_id, target_folder).await?;
        let name = saved.display_name();
        repo::update_saved_request(
            self.pool(),
            saved_id,
            &name,
            &saved.request,
            target_folder,
            &now_rfc3339(),
        )
        .await?;
        repo::set_request_order(self.pool(), saved_id, order).await?;
        Ok(())
    }

    /// Reorder requests within one collection by assigning `sort_order` by index.
    pub async fn reorder_requests(&self, ordered_ids: &[Uuid]) -> Result<(), CollectionError> {
        repo::reorder_requests(self.pool(), ordered_ids).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{HttpMethod, Request};

    async fn service() -> CollectionService {
        let storage = Storage::open_in_memory().await.unwrap();
        CollectionService::new(storage)
    }

    #[tokio::test]
    async fn creates_and_lists_collections() {
        let service = service().await;
        let created = service.create_collection("  My APIs  ").await.unwrap();
        assert_eq!(created.name, "My APIs");
        let trees = service.list_trees().await.unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].collection.name, "My APIs");
    }

    #[tokio::test]
    async fn rejects_empty_names() {
        let service = service().await;
        assert!(matches!(
            service.create_collection("   ").await,
            Err(CollectionError::EmptyName)
        ));
    }

    #[tokio::test]
    async fn nested_folders_and_depth_limit() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let mut parent = None;
        for level in 1..=MAX_FOLDER_DEPTH {
            let folder = service
                .create_folder(collection.id, parent, &format!("Level {level}"))
                .await
                .unwrap();
            parent = Some(folder.id);
        }
        // One level too deep must be refused.
        assert!(matches!(
            service
                .create_folder(collection.id, parent, "Too deep")
                .await,
            Err(CollectionError::TooDeep)
        ));
        let tree = service.tree(collection.id).await.unwrap();
        let mut node = &tree.root;
        let mut depth = 0;
        while let Some(child) = node.children.first() {
            node = child;
            depth += 1;
        }
        assert_eq!(depth, MAX_FOLDER_DEPTH);
    }

    #[tokio::test]
    async fn saves_updates_and_deletes_requests() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let folder = service
            .create_folder(collection.id, None, "Users")
            .await
            .unwrap();
        let request = Request::new(HttpMethod::Get, "https://api.example.com/users");
        let saved = service
            .save_request(collection.id, Some(folder.id), "List users", &request)
            .await
            .unwrap();
        assert_eq!(saved.folder_id, Some(folder.id));

        let mut renamed = request.clone();
        renamed.url = "https://api.example.com/v2/users".into();
        service
            .update_saved_request(saved.id, "List users v2", &renamed)
            .await
            .unwrap();
        let reloaded = service.saved_request(saved.id).await.unwrap();
        assert_eq!(reloaded.name, "List users v2");
        assert_eq!(reloaded.request.url, "https://api.example.com/v2/users");
        assert_eq!(reloaded.folder_id, Some(folder.id));

        let copy = service.duplicate_saved_request(saved.id).await.unwrap();
        assert_eq!(copy.name, "List users v2 (copy)");

        service.delete_saved_request(saved.id).await.unwrap();
        assert!(service.saved_request(saved.id).await.is_err());
    }

    #[tokio::test]
    async fn moving_a_folder_into_itself_is_refused() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let parent = service
            .create_folder(collection.id, None, "Parent")
            .await
            .unwrap();
        let child = service
            .create_folder(collection.id, Some(parent.id), "Child")
            .await
            .unwrap();
        assert!(matches!(
            service.move_folder(parent.id, Some(child.id)).await,
            Err(CollectionError::InvalidMove)
        ));
        service.move_folder(child.id, None).await.unwrap();
        let tree = service.tree(collection.id).await.unwrap();
        assert_eq!(tree.root.children.len(), 2);
    }

    #[tokio::test]
    async fn moves_requests_between_folders() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let a = service
            .create_folder(collection.id, None, "A")
            .await
            .unwrap();
        let b = service
            .create_folder(collection.id, None, "B")
            .await
            .unwrap();
        let request = Request::new(HttpMethod::Get, "https://example.com");
        let saved = service
            .save_request(collection.id, Some(a.id), "Ping", &request)
            .await
            .unwrap();
        service
            .move_saved_request(saved.id, Some(b.id))
            .await
            .unwrap();
        let moved = service.saved_request(saved.id).await.unwrap();
        assert_eq!(moved.folder_id, Some(b.id));
        let tree = service.tree(collection.id).await.unwrap();
        assert_eq!(tree.requests_in(Some(a.id)).len(), 0);
        assert_eq!(tree.requests_in(Some(b.id)).len(), 1);
    }

    #[tokio::test]
    async fn deleting_a_collection_removes_children() {
        let service = service().await;
        let collection = service.create_collection("Temp").await.unwrap();
        let folder = service
            .create_folder(collection.id, None, "F")
            .await
            .unwrap();
        service
            .save_request(
                collection.id,
                Some(folder.id),
                "R",
                &Request::new(HttpMethod::Get, "https://example.com"),
            )
            .await
            .unwrap();
        service.delete_collection(collection.id).await.unwrap();
        assert!(service.list_trees().await.unwrap().is_empty());
        assert!(service.saved_request(Uuid::new_v4()).await.is_err());
    }

    #[tokio::test]
    async fn duplicates_a_collection_with_its_contents() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let folder = service
            .create_folder(collection.id, None, "Users")
            .await
            .unwrap();
        service
            .save_request(
                collection.id,
                Some(folder.id),
                "List",
                &Request::new(HttpMethod::Get, "https://example.com/users"),
            )
            .await
            .unwrap();
        let copy = service
            .duplicate_collection(collection.id, "APIs copy")
            .await
            .unwrap();
        let tree = service.tree(copy.id).await.unwrap();
        assert_eq!(tree.all_requests().len(), 1);
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].name(), "Users");
    }

    #[tokio::test]
    async fn reorder_assigns_new_sort_orders() {
        let service = service().await;
        let collection = service.create_collection("APIs").await.unwrap();
        let first = service
            .save_request(
                collection.id,
                None,
                "First",
                &Request::new(HttpMethod::Get, "https://example.com/1"),
            )
            .await
            .unwrap();
        let second = service
            .save_request(
                collection.id,
                None,
                "Second",
                &Request::new(HttpMethod::Get, "https://example.com/2"),
            )
            .await
            .unwrap();
        service
            .reorder_requests(&[second.id, first.id])
            .await
            .unwrap();
        let tree = service.tree(collection.id).await.unwrap();
        let names: Vec<&str> = tree.root.requests.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Second", "First"]);
    }
}
