//! Authentifizierungs-Bausteine der Control-Plane.
//!
//! Jedes Untermodul kapselt genau eine etablierte Bibliothek — es wird
//! keine eigene Kryptografie implementiert:
//! - [`password`][]: Argon2id-Passwort-Hashing (`argon2`-Crate)
//! - [`jwt`][]: HS256-JWT-Access-Tokens (`jsonwebtoken`-Crate)
//! - [`token`]: widerrufbare Opak-Tokens für Refresh-Sessions/API-Keys (SHA-256)
//! - [`totp`]: RFC-6238-TOTP für optionale Zwei-Faktor-Authentifizierung (`totp-rs`-Crate)

pub mod jwt;
pub mod password;
pub mod token;
pub mod totp;
