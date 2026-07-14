//! LedgerZero routing server + runtime backend application server.
//!
//! M1 walking skeleton (Impl Plan): routing, Google OAuth authentication,
//! sessions with refresh rotation, bootstrap-owner authorization, operational
//! audit log. Accounting APIs arrive in M4 behind this boundary.

pub mod app;
pub mod audit;
pub mod auth;
pub mod auth_provider;
pub mod authz;
pub mod books;
pub mod books_api;
pub mod config;
pub mod dev_artifacts;
pub mod error;
pub mod sessions;
pub mod state;
pub mod users;
