//! Passwort-Hashing für Admin-UI/API- und Mail-Authentifizierung.
//!
//! Ausschließlich Argon2id (RFC 9106-empfohlene Parameter über die
//! `argon2`-Crate-Defaults) — keine Eigenimplementierung von Krypto-
//! Primitiven. Der resultierende PHC-String wird direkt in `users.password_hash`
//! bzw. `api_tokens.token_hash` gespeichert und ist selbstbeschreibend
//! (Algorithmus + Parameter + Salt eingebettet), sodass spätere
//! Parameteränderungen ohne Migrationsaufwand möglich sind.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Passwort-Hashing fehlgeschlagen: {0}")]
    Hash(String),
    #[error("Passwort-Hash konnte nicht geparst werden: {0}")]
    InvalidHash(String),
}

/// Hasht ein Klartextpasswort mit Argon2id und gibt den PHC-formatierten
/// String zurück (enthält Salt und Parameter, kein separates Salt-Feld nötig).
pub fn hash_password(plaintext: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(hash.to_string())
}

/// Prüft ein Klartextpasswort gegen einen gespeicherten Argon2id-Hash.
///
/// Gibt bei strukturell ungültigem Hash `Ok(false)` zurück, nicht `Err` —
/// so verhält sich ein defekter Datensatz wie ein falsches Passwort statt
/// eine unterscheidbare Fehlermeldung zu liefern (Schutz vor Enumeration
/// über Fehlerverhalten).
pub fn verify_password(plaintext: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_correct_password() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
    }

    #[test]
    fn rejects_wrong_password() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn same_password_produces_different_hashes() {
        // unterschiedliches Salt pro Aufruf -> kein deterministischer Hash
        let a = hash_password("same-input").unwrap();
        let b = hash_password("same-input").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_stored_hash_is_treated_as_mismatch() {
        assert!(!verify_password("anything", "not-a-valid-phc-string"));
    }

    #[test]
    fn hash_uses_argon2id_identifier() {
        let hash = hash_password("x").unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }
}
