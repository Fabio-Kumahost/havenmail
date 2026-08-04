//! Havenmail Core — geteilte Bibliothek für Auth, RBAC, Audit-Log und
//! Datenbankzugriff der Control-Plane.
//!
//! Enthält bewusst KEINEN eigenen SMTP-/IMAP-/JMAP-/TLS-Code — diese Crate
//! orchestriert nur (Config-Rendering, DB-Zugriff, Auth-Primitive über
//! etablierte Bibliotheken). Siehe docs/architecture.md im Repo-Root.

pub mod audit;
pub mod auth;
pub mod config_render;
pub mod db;
pub mod rbac;

pub use rbac::{Action, Actor, Role};
