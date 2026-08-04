//! Optionale TOTP-Zwei-Faktor-Authentifizierung.
//!
//! Nutzt die `totp-rs`-Crate (RFC-6238-konform) — keine Eigenimplementierung
//! des HOTP/TOTP-Algorithmus. Das Secret wird hier nur erzeugt/geprüft;
//! die Verschlüsselung vor dem Speichern in `users.totp_secret_enc`
//! übernimmt das Secret-Handling der Control-Plane (AEAD, siehe
//! docs/architecture.md, Datenmodell).

use thiserror::Error;
use totp_rs::{Algorithm, Secret, TOTP};

#[derive(Debug, Error)]
pub enum TotpError {
    #[error("TOTP-Secret konnte nicht erzeugt werden: {0}")]
    SecretGeneration(String),
    #[error("Ungültiges TOTP-Secret: {0}")]
    InvalidSecret(String),
}

/// Erzeugt ein neues zufälliges TOTP-Secret (Base32-kodiert) für einen
/// Benutzer sowie die zugehörige `otpauth://`-URI für QR-Code-Anzeige.
pub fn generate_secret(account_email: &str, issuer: &str) -> Result<(String, String), TotpError> {
    let secret = Secret::generate_secret();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret
            .to_bytes()
            .map_err(|e| TotpError::SecretGeneration(e.to_string()))?,
        Some(issuer.to_string()),
        account_email.to_string(),
    )
    .map_err(|e| TotpError::SecretGeneration(e.to_string()))?;

    let base32_secret = secret.to_encoded().to_string();
    let uri = totp.get_url();
    Ok((base32_secret, uri))
}

/// Prüft einen vom Nutzer eingegebenen 6-stelligen Code gegen das
/// gespeicherte (entschlüsselte) Base32-Secret, mit Toleranz von ±1
/// Zeitfenster (Standardverhalten der Bibliothek über `skew`).
pub fn verify_code(base32_secret: &str, code: &str) -> Result<bool, TotpError> {
    let secret_bytes = Secret::Encoded(base32_secret.to_string())
        .to_bytes()
        .map_err(|e| TotpError::InvalidSecret(e.to_string()))?;
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        None,
        "".to_string(),
    )
    .map_err(|e| TotpError::InvalidSecret(e.to_string()))?;
    Ok(totp.check_current(code).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_secret_and_verifies_current_code() {
        let (secret, uri) = generate_secret("admin@example.org", "Havenmail").unwrap();
        assert!(uri.starts_with("otpauth://totp/"));

        // Code für das aktuelle Zeitfenster selbst erzeugen, um die
        // Verifikation ohne externe Abhängigkeit von der Systemuhr zu testen.
        let secret_bytes = Secret::Encoded(secret.clone()).to_bytes().unwrap();
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            None,
            "".to_string(),
        )
        .unwrap();
        let code = totp.generate_current().unwrap();

        assert!(verify_code(&secret, &code).unwrap());
    }

    #[test]
    fn rejects_wrong_code() {
        let (secret, _) = generate_secret("admin@example.org", "Havenmail").unwrap();
        assert!(!verify_code(&secret, "000000").unwrap());
    }

    #[test]
    fn rejects_malformed_secret() {
        assert!(verify_code("not-base32!!", "123456").is_err());
    }
}
