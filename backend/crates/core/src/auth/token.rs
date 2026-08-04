//! Widerrufbare Opak-Tokens für Refresh-Sessions und API-Keys.
//!
//! Anders als Access-JWTs (siehe `super::jwt`) werden diese Tokens serverseitig
//! als SHA-256-Hash gespeichert (`sessions.refresh_token_hash`,
//! `api_tokens.token_hash`) und können jederzeit widerrufen werden. Das
//! Klartext-Token existiert nur einmal beim Erstellen und wird danach nie
//! wieder ausgegeben — verloren bedeutet neu ausstellen, nicht wiederherstellen.

use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

/// Anzahl zufälliger Bytes je Token vor Base64-Kodierung (256 Bit Entropie).
const TOKEN_BYTES: usize = 32;

/// Erzeugt ein neues zufälliges Opak-Token (z. B. für Refresh-Session oder
/// App-Passwort/API-Key) und gibt `(plaintext, sha256_hash_hex)` zurück.
/// Nur `plaintext` wird an den Client ausgegeben, nur der Hash gespeichert.
pub fn generate_opaque_token() -> (String, String) {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let plaintext = format!("hvm_{}", hex::encode(bytes));
    let hash = hash_token(&plaintext);
    (plaintext, hash)
}

/// Hasht ein vom Client vorgelegtes Token zum Vergleich mit dem gespeicherten
/// Hash. SHA-256 genügt hier (kein Passwort mit niedriger Entropie, sondern
/// bereits 256 Bit Zufall) — im Gegensatz zu Nutzerpasswörtern braucht ein
/// Opak-Token kein absichtlich langsames KDF.
pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// Konstante-Zeit-Vergleich zweier Hash-Hexstrings (Schutz vor Timing-Angriffen
/// bei der Token-Prüfung).
pub fn verify_token(plaintext: &str, stored_hash: &str) -> bool {
    let computed = hash_token(plaintext);
    constant_time_eq(computed.as_bytes(), stored_hash.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Minimaler Hex-Encoder, um keine zusätzliche Abhängigkeit für reines
/// Hex-Encoding einzuführen (sha2 liefert bereits `GenericArray<u8>`).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        let (a, _) = generate_opaque_token();
        let (b, _) = generate_opaque_token();
        assert_ne!(a, b);
    }

    #[test]
    fn verify_accepts_matching_token() {
        let (plaintext, hash) = generate_opaque_token();
        assert!(verify_token(&plaintext, &hash));
    }

    #[test]
    fn verify_rejects_wrong_token() {
        let (_, hash) = generate_opaque_token();
        assert!(!verify_token("hvm_wrong", &hash));
    }

    #[test]
    fn token_has_expected_prefix_and_length() {
        let (plaintext, _) = generate_opaque_token();
        assert!(plaintext.starts_with("hvm_"));
        assert_eq!(plaintext.len(), 4 + TOKEN_BYTES * 2);
    }
}
