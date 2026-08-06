//! Generisches AES-256-GCM-Verschlüsseln/-Entschlüsseln von Klartext-Secrets
//! vor der Ablage in Postgres (DKIM-Privatschlüssel in
//! `dkim_keys.private_key_enc`, TOTP-Secrets in `users.totp_secret_enc`).
//!
//! `master_key` kommt in beiden Fällen aus `HAVENMAIL_SECRETS_KEY` (vom
//! Installer generiert, 32 Byte). War ursprünglich zweimal fast identisch
//! in `dkim.rs` implementiert — hierher gezogen, damit beide Stellen exakt
//! dasselbe Format (`nonce || ciphertext`) und dieselbe Fehlerbehandlung
//! teilen statt potenziell auseinanderzudriften.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use thiserror::Error;

/// Länge des AES-GCM-Nonce in Bytes (Standard für AES-GCM).
const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum SecretsCryptoError {
    #[error("Verschlüsselung fehlgeschlagen")]
    Encryption,
    #[error("Entschlüsselung fehlgeschlagen (falscher Schlüssel oder manipulierte Daten)")]
    Decryption,
    #[error("Master-Schlüssel muss genau 32 Byte lang sein")]
    InvalidMasterKey,
}

/// Verschlüsselt `plaintext` mit AES-256-GCM. Rückgabe: `nonce || ciphertext`.
pub fn encrypt(master_key: &[u8], plaintext: &str) -> Result<Vec<u8>, SecretsCryptoError> {
    if master_key.len() != 32 {
        return Err(SecretsCryptoError::InvalidMasterKey);
    }
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| SecretsCryptoError::Encryption)?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Entschlüsselt einen mit [`encrypt`] verschlüsselten Blob.
pub fn decrypt(master_key: &[u8], blob: &[u8]) -> Result<String, SecretsCryptoError> {
    if master_key.len() != 32 {
        return Err(SecretsCryptoError::InvalidMasterKey);
    }
    if blob.len() < NONCE_LEN {
        return Err(SecretsCryptoError::Decryption);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| SecretsCryptoError::Decryption)?;
    String::from_utf8(plaintext).map_err(|_| SecretsCryptoError::Decryption)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master_key() -> Vec<u8> {
        vec![7u8; 32]
    }

    #[test]
    fn encrypt_then_decrypt_roundtrips() {
        let blob = encrypt(&master_key(), "top secret").unwrap();
        assert_eq!(decrypt(&master_key(), &blob).unwrap(), "top secret");
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let blob = encrypt(&master_key(), "top secret").unwrap();
        let wrong_key = vec![9u8; 32];
        assert!(decrypt(&wrong_key, &blob).is_err());
    }

    #[test]
    fn rejects_master_key_of_wrong_length() {
        assert!(encrypt(&[1u8; 16], "x").is_err());
        assert!(decrypt(&[1u8; 16], &[0u8; 20]).is_err());
    }

    #[test]
    fn each_encryption_uses_a_fresh_nonce() {
        let a = encrypt(&master_key(), "same plaintext").unwrap();
        let b = encrypt(&master_key(), "same plaintext").unwrap();
        assert_ne!(a, b);
    }
}
