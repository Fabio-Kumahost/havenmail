//! DKIM-Schlüsselerzeugung und -Verschlüsselung.
//!
//! Erzeugung über die etablierte `rsa`-Crate (RSA-2048, wie von Rspamd und
//! allen gängigen Empfängern unterstützt) — keine eigene Schlüsselerzeugung
//! oder Signaturlogik; das eigentliche Signieren übernimmt Rspamd
//! (siehe config/rspamd/local.d/dkim_signing.conf.tera). Private Schlüssel
//! werden ausschließlich verschlüsselt (AES-256-GCM) in `dkim_keys.private_key_enc`
//! abgelegt.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use thiserror::Error;

const RSA_KEY_BITS: usize = 2048;
/// Länge des AES-GCM-Nonce in Bytes (Standard für AES-GCM).
const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum DkimError {
    #[error("Schlüsselerzeugung fehlgeschlagen: {0}")]
    Generation(String),
    #[error("Verschlüsselung fehlgeschlagen")]
    Encryption,
    #[error("Entschlüsselung fehlgeschlagen (falscher Schlüssel oder manipulierte Daten)")]
    Decryption,
    #[error("Master-Schlüssel muss genau 32 Byte lang sein")]
    InvalidMasterKey,
}

pub struct GeneratedDkimKey {
    /// PKCS#8-PEM des privaten Schlüssels — nur zum sofortigen Verschlüsseln
    /// verwenden, niemals unverschlüsselt persistieren.
    pub private_key_pem: String,
    /// Fertiger DNS-TXT-Record-Wert für `<selector>._domainkey.<domain>`.
    pub dns_txt_value: String,
}

/// Erzeugt ein neues RSA-2048-Schlüsselpaar und den passenden DKIM-DNS-Eintrag.
pub fn generate_dkim_key() -> Result<GeneratedDkimKey, DkimError> {
    let mut rng = rsa::rand_core::OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, RSA_KEY_BITS)
        .map_err(|e| DkimError::Generation(e.to_string()))?;
    let public_key = RsaPublicKey::from(&private_key);

    let private_key_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| DkimError::Generation(e.to_string()))?
        .to_string();

    let public_key_der = public_key
        .to_pkcs1_der()
        .map_err(|e| DkimError::Generation(e.to_string()))?;
    let public_key_b64 = STANDARD.encode(public_key_der.as_bytes());

    Ok(GeneratedDkimKey {
        private_key_pem,
        dns_txt_value: format!("v=DKIM1; k=rsa; p={public_key_b64}"),
    })
}

/// Verschlüsselt den PEM-Text eines privaten Schlüssels mit AES-256-GCM.
/// `master_key` muss exakt 32 Byte lang sein (aus `HAVENMAIL_SECRETS_KEY`,
/// vom Installer generiert). Rückgabe: `nonce || ciphertext`, so wie es in
/// `dkim_keys.private_key_enc` gespeichert wird.
pub fn encrypt_private_key(master_key: &[u8], private_key_pem: &str) -> Result<Vec<u8>, DkimError> {
    if master_key.len() != 32 {
        return Err(DkimError::InvalidMasterKey);
    }
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, private_key_pem.as_bytes())
        .map_err(|_| DkimError::Encryption)?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Entschlüsselt einen mit [`encrypt_private_key`] verschlüsselten Blob.
pub fn decrypt_private_key(master_key: &[u8], blob: &[u8]) -> Result<String, DkimError> {
    if master_key.len() != 32 {
        return Err(DkimError::InvalidMasterKey);
    }
    if blob.len() < NONCE_LEN {
        return Err(DkimError::Decryption);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| DkimError::Decryption)?;
    String::from_utf8(plaintext).map_err(|_| DkimError::Decryption)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master_key() -> Vec<u8> {
        vec![7u8; 32]
    }

    #[test]
    fn generates_valid_pem_and_dns_value() {
        let key = generate_dkim_key().unwrap();
        assert!(key
            .private_key_pem
            .starts_with("-----BEGIN PRIVATE KEY-----"));
        assert!(key.dns_txt_value.starts_with("v=DKIM1; k=rsa; p="));
    }

    #[test]
    fn encrypt_then_decrypt_roundtrips() {
        let key = generate_dkim_key().unwrap();
        let encrypted = encrypt_private_key(&master_key(), &key.private_key_pem).unwrap();
        let decrypted = decrypt_private_key(&master_key(), &encrypted).unwrap();
        assert_eq!(decrypted, key.private_key_pem);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let key = generate_dkim_key().unwrap();
        let encrypted = encrypt_private_key(&master_key(), &key.private_key_pem).unwrap();
        let wrong_key = vec![9u8; 32];
        assert!(decrypt_private_key(&wrong_key, &encrypted).is_err());
    }

    #[test]
    fn rejects_master_key_with_wrong_length() {
        let short_key = vec![1u8; 16];
        assert!(matches!(
            encrypt_private_key(&short_key, "irrelevant"),
            Err(DkimError::InvalidMasterKey)
        ));
    }

    #[test]
    fn two_generated_keys_are_different() {
        let a = generate_dkim_key().unwrap();
        let b = generate_dkim_key().unwrap();
        assert_ne!(a.private_key_pem, b.private_key_pem);
    }
}
