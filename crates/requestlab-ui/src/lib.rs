//! RequestLab's native GPUI interface.
//!
//! The root [`AppView`] owns application state; panels are implemented as
//! `impl AppView` blocks in their own modules.

pub mod actions;
pub mod app_view;
pub mod components;
pub mod dialogs;
pub mod editor;
pub mod editor_state;
pub mod input;
pub mod json_tree;
pub mod response_panel;
pub mod sidebar;
pub mod theme;
pub mod toolbar;

pub use app_view::{bind_keys, AppView, Dialog, ResponseTab, SidebarSection};
pub use editor_state::{AuthKind, EditorTab, RequestEditor, SavedLocation};
pub use input::TextInput;
pub use theme::{Theme, ThemeMode};
