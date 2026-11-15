//! Collection management: collections, nested folders and saved requests.
//!
//! This crate sits on top of the storage repository layer. It enforces the
//! business rules (nesting depth, name requirements, move safety) so the UI can
//! stay a thin presentation layer.

pub mod errors;
pub mod folders;
pub mod manager;

pub use errors::CollectionError;
pub use manager::CollectionService;
