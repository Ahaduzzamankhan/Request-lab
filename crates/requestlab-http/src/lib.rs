//! HTTP execution engine: request preparation, authentication, async execution.
//!
//! This crate is independent of GPUI and of the database. It turns a user
//! [`Request`] plus a resolved environment into a real network round trip and
//! returns a fully captured [`HttpResponse`].

pub mod authentication;
pub mod builder;
pub mod client;
pub mod errors;
pub mod response;
pub mod timing;

pub use builder::{prepare_request, PreparedRequest, ResolvedBody, SendOptions};
pub use client::{cancellation_channel, Canceller, HttpClient, HttpConfig};
pub use errors::HttpError;
pub use timing::RequestTimer;
