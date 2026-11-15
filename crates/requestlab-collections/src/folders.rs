//! Folder tree helpers: flattening, subtree height and cycle detection.

use requestlab_models::{CollectionNode, CollectionTree, Folder};
use uuid::Uuid;

/// Every folder in the tree, parents before children.
pub fn collect_folders(node: &CollectionNode) -> Vec<&Folder> {
    let mut out: Vec<&Folder> = node.folder.iter().collect();
    for child in &node.children {
        out.extend(collect_folders(child));
    }
    out
}

/// Height of the folder subtree rooted at `folder_id` (1 for a leaf).
pub fn subtree_height(tree: &CollectionTree, folder_id: Uuid) -> usize {
    fn find(node: &CollectionNode, target: Uuid) -> Option<&CollectionNode> {
        if node.folder_id() == Some(target) {
            return Some(node);
        }
        node.children.iter().find_map(|child| find(child, target))
    }
    fn height(node: &CollectionNode) -> usize {
        1 + node.children.iter().map(height).max().unwrap_or(0)
    }
    find(&tree.root, folder_id).map(height).unwrap_or(1)
}

/// Whether moving `folder_id` under `new_parent` would create a cycle.
pub fn would_create_cycle(tree: &CollectionTree, folder_id: Uuid, new_parent: Uuid) -> bool {
    folder_id == new_parent || tree.folder_is_descendant(new_parent, folder_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{build_tree, Collection};
    use uuid::Uuid;

    fn setup() -> (CollectionTree, Folder, Folder, Folder) {
        let collection = Collection {
            id: Uuid::new_v4(),
            name: "C".into(),
            description: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let make = |name: &str, parent: Option<Uuid>| Folder {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            parent_id: parent,
            name: name.into(),
            sort_order: 0,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let root = make("Root", None);
        let mid = make("Mid", Some(root.id));
        let leaf = make("Leaf", Some(mid.id));
        let tree = build_tree(
            collection,
            vec![root.clone(), mid.clone(), leaf.clone()],
            vec![],
        );
        (tree, root, mid, leaf)
    }

    #[test]
    fn flattens_every_folder() {
        let (tree, _, _, _) = setup();
        assert_eq!(collect_folders(&tree.root).len(), 3);
    }

    #[test]
    fn measures_subtree_height() {
        let (tree, root, mid, leaf) = setup();
        assert_eq!(subtree_height(&tree, root.id), 3);
        assert_eq!(subtree_height(&tree, mid.id), 2);
        assert_eq!(subtree_height(&tree, leaf.id), 1);
    }

    #[test]
    fn detects_cycles() {
        let (tree, root, mid, _) = setup();
        assert!(would_create_cycle(&tree, root.id, root.id));
        assert!(would_create_cycle(&tree, root.id, mid.id));
        assert!(!would_create_cycle(&tree, mid.id, root.id));
    }
}
