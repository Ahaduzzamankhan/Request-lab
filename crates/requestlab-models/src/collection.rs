//! Collections, folders and saved requests, including tree construction.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::request::Request;

/// A named group of saved requests, the top level of the collection tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A folder inside a collection. `parent_id` is `None` for collection-level folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A request saved into a collection or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub folder_id: Option<Uuid>,
    pub name: String,
    pub request: Request,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl SavedRequest {
    /// The label shown in the sidebar and tabs.
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.request.display_name()
        } else {
            self.name.clone()
        }
    }
}

/// A node in the rendered collection tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionNode {
    /// `None` for the collection root.
    pub folder: Option<Folder>,
    pub requests: Vec<SavedRequest>,
    pub children: Vec<CollectionNode>,
}

impl CollectionNode {
    pub fn root() -> Self {
        Self {
            folder: None,
            requests: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn folder_id(&self) -> Option<Uuid> {
        self.folder.as_ref().map(|folder| folder.id)
    }

    pub fn name(&self) -> &str {
        self.folder
            .as_ref()
            .map(|folder| folder.name.as_str())
            .unwrap_or("Root")
    }

    /// Depth of this node where the collection root is depth 0.
    pub fn depth(&self) -> usize {
        match &self.folder {
            None => 0,
            Some(_) => 1,
        }
    }

    /// Total number of requests in this subtree.
    pub fn request_count(&self) -> usize {
        self.requests.len() + self.children.iter().map(Self::request_count).sum::<usize>()
    }

    /// Iterate every request in this subtree, parents first.
    pub fn walk_requests(&self) -> Vec<&SavedRequest> {
        let mut out: Vec<&SavedRequest> = self.requests.iter().collect();
        for child in &self.children {
            out.extend(child.walk_requests());
        }
        out
    }
}

/// A collection together with its folder/request hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionTree {
    pub collection: Collection,
    pub root: CollectionNode,
}

impl CollectionTree {
    /// Every request in this collection, flattened.
    pub fn all_requests(&self) -> Vec<&SavedRequest> {
        self.root.walk_requests()
    }

    pub fn find_request(&self, id: Uuid) -> Option<&SavedRequest> {
        self.all_requests().into_iter().find(|saved| saved.id == id)
    }

    pub fn folder(&self, id: Uuid) -> Option<&Folder> {
        fn search(node: &CollectionNode, id: Uuid) -> Option<&Folder> {
            if node.folder.as_ref().is_some_and(|folder| folder.id == id) {
                return node.folder.as_ref();
            }
            node.children.iter().find_map(|child| search(child, id))
        }
        search(&self.root, id)
    }

    /// Requests stored directly in `folder_id`, with `None` meaning the root.
    pub fn requests_in(&self, folder_id: Option<Uuid>) -> Vec<&SavedRequest> {
        fn collect<'a>(
            node: &'a CollectionNode,
            target: Option<Uuid>,
            out: &mut Vec<&'a SavedRequest>,
        ) {
            if node.folder_id() == target {
                out.extend(node.requests.iter());
            }
            for child in &node.children {
                collect(child, target, out);
            }
        }
        let mut out = Vec::new();
        collect(&self.root, folder_id, &mut out);
        out
    }

    /// Whether `candidate` is inside the subtree rooted at `ancestor`.
    pub fn folder_is_descendant(&self, candidate: Uuid, ancestor: Uuid) -> bool {
        fn search(node: &CollectionNode, candidate: Uuid, ancestor: Uuid, inside: bool) -> bool {
            let inside = inside || node.folder_id() == Some(ancestor);
            if inside && node.folder_id() == Some(candidate) {
                return true;
            }
            node.children
                .iter()
                .any(|child| search(child, candidate, ancestor, inside))
        }
        search(&self.root, candidate, ancestor, false)
    }
}

/// Build a tree from flat rows.
///
/// Rows whose parent is missing are attached to the root so that no saved
/// request can become unreachable.
pub fn build_tree(
    collection: Collection,
    folders: Vec<Folder>,
    requests: Vec<SavedRequest>,
) -> CollectionTree {
    let mut sorted_folders = folders;
    sorted_folders.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));

    let mut sorted_requests = requests;
    sorted_requests.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));

    let mut root = CollectionNode::root();
    let mut nodes: Vec<CollectionNode> = sorted_folders
        .into_iter()
        .map(|folder| CollectionNode {
            folder: Some(folder),
            requests: Vec::new(),
            children: Vec::new(),
        })
        .collect();

    // Attach requests to their folder, or to the root when the folder is gone.
    let mut orphans = Vec::new();
    for request in sorted_requests {
        match request.folder_id {
            None => root.requests.push(request),
            Some(folder_id) => match nodes
                .iter_mut()
                .find(|node| node.folder_id() == Some(folder_id))
            {
                Some(node) => node.requests.push(request),
                None => {
                    let mut request = request;
                    request.folder_id = None;
                    orphans.push(request);
                }
            },
        }
    }
    root.requests.extend(orphans);
    root.requests
        .sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));

    // Attach folders to parents. Doing this by depth keeps ordering stable even
    // when rows arrive in an arbitrary order.
    let mut pending = nodes;
    let mut progressed = true;
    while progressed && !pending.is_empty() {
        progressed = false;
        let mut still_pending = Vec::new();
        for node in pending {
            let parent_id = node.folder.as_ref().and_then(|folder| folder.parent_id);
            match parent_id {
                None => {
                    root.children.push(node);
                    progressed = true;
                }
                Some(parent_id) => {
                    let target = find_node_mut(&mut root, parent_id);
                    match target {
                        Some(parent) => {
                            parent.children.push(node);
                            progressed = true;
                        }
                        None => {
                            // Parent may still be pending in this pass.
                            still_pending.push(node);
                        }
                    }
                }
            }
        }
        pending = still_pending;
        if !progressed {
            break;
        }
    }
    // Anything left has a parent that is missing or part of a cycle: surface at root.
    for mut node in pending {
        if let Some(folder) = node.folder.as_mut() {
            folder.parent_id = None;
        }
        root.children.push(node);
    }

    sort_children(&mut root);
    CollectionTree { collection, root }
}

fn find_node_mut(node: &mut CollectionNode, id: Uuid) -> Option<&mut CollectionNode> {
    if node.folder_id() == Some(id) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, id) {
            return Some(found);
        }
    }
    None
}

fn sort_children(node: &mut CollectionNode) {
    node.children.sort_by(|a, b| {
        let a = a.folder.as_ref();
        let b = b.folder.as_ref();
        let a_order = a.map(|folder| folder.sort_order).unwrap_or(0);
        let b_order = b.map(|folder| folder.sort_order).unwrap_or(0);
        a_order
            .cmp(&b_order)
            .then_with(|| a.map(|f| &f.name).cmp(&b.map(|f| &f.name)))
    });
    for child in &mut node.children {
        sort_children(child);
    }
}

/// Depth of a folder, where a collection-level folder has depth 1.
pub fn folder_depth(folders: &[Folder], folder_id: Uuid) -> usize {
    let mut depth = 1;
    let mut current = folders.iter().find(|folder| folder.id == folder_id);
    let mut guard = 0;
    while let Some(folder) = current {
        match folder.parent_id {
            Some(parent_id) => {
                depth += 1;
                current = folders.iter().find(|f| f.id == parent_id);
                guard += 1;
                if guard > 64 {
                    break;
                }
            }
            None => break,
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{HttpMethod, Request};

    fn collection() -> Collection {
        Collection {
            id: Uuid::new_v4(),
            name: "My APIs".into(),
            description: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn folder(collection_id: Uuid, name: &str, parent_id: Option<Uuid>, order: i64) -> Folder {
        Folder {
            id: Uuid::new_v4(),
            collection_id,
            parent_id,
            name: name.into(),
            sort_order: order,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn saved(collection_id: Uuid, name: &str, folder_id: Option<Uuid>, order: i64) -> SavedRequest {
        SavedRequest {
            id: Uuid::new_v4(),
            collection_id,
            folder_id,
            name: name.into(),
            request: Request::new(HttpMethod::Get, "https://example.com"),
            sort_order: order,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn builds_nested_tree_and_preserves_order() {
        let collection = collection();
        let users = folder(collection.id, "Users", None, 1);
        let admin = folder(collection.id, "Admin", Some(users.id), 1);
        let auth = folder(collection.id, "Auth", None, 2);
        let request_root = saved(collection.id, "Health", None, 2);
        let request_users = saved(collection.id, "List users", Some(users.id), 1);
        let request_admin = saved(collection.id, "Ban user", Some(admin.id), 1);

        let tree = build_tree(
            collection.clone(),
            vec![auth.clone(), admin.clone(), users.clone()],
            vec![
                request_admin.clone(),
                request_root.clone(),
                request_users.clone(),
            ],
        );

        assert_eq!(tree.root.children.len(), 2);
        assert_eq!(tree.root.children[0].name(), "Users");
        assert_eq!(tree.root.children[1].name(), "Auth");
        assert_eq!(tree.root.children[0].children[0].name(), "Admin");
        assert_eq!(tree.all_requests().len(), 3);
        assert_eq!(tree.root.requests[0].name, "Health");
        assert_eq!(tree.requests_in(Some(users.id))[0].name, "List users");
        assert!(tree.folder_is_descendant(admin.id, users.id));
        assert!(!tree.folder_is_descendant(users.id, admin.id));
        assert_eq!(
            tree.folder(admin.id).map(|f| f.name.as_str()),
            Some("Admin")
        );
        assert_eq!(
            tree.find_request(request_admin.id).unwrap().name,
            "Ban user"
        );
    }

    #[test]
    fn orphaned_requests_are_recovered_to_root() {
        let collection = collection();
        let missing_folder = Uuid::new_v4();
        let orphan = saved(collection.id, "Orphan", Some(missing_folder), 1);
        let tree = build_tree(collection, vec![], vec![orphan]);
        assert_eq!(tree.root.requests.len(), 1);
        assert_eq!(tree.root.requests[0].folder_id, None);
    }

    #[test]
    fn orphaned_folders_fall_back_to_root() {
        let collection = collection();
        let child = folder(collection.id, "Child", Some(Uuid::new_v4()), 1);
        let tree = build_tree(collection, vec![child], vec![]);
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(
            tree.root.children[0].folder.as_ref().unwrap().parent_id,
            None
        );
    }

    #[test]
    fn folder_depth_is_measured_from_the_collection_root() {
        let collection = collection();
        let a = folder(collection.id, "A", None, 1);
        let b = folder(collection.id, "B", Some(a.id), 1);
        let c = folder(collection.id, "C", Some(b.id), 1);
        let folders = vec![a.clone(), b.clone(), c.clone()];
        assert_eq!(folder_depth(&folders, a.id), 1);
        assert_eq!(folder_depth(&folders, b.id), 2);
        assert_eq!(folder_depth(&folders, c.id), 3);
    }

    #[test]
    fn request_counts_include_children() {
        let collection = collection();
        let users = folder(collection.id, "Users", None, 1);
        let tree = build_tree(
            collection.clone(),
            vec![users.clone()],
            vec![
                saved(collection.id, "A", Some(users.id), 1),
                saved(collection.id, "B", None, 1),
            ],
        );
        assert_eq!(tree.root.request_count(), 2);
        assert_eq!(tree.root.children[0].request_count(), 1);
    }
}
