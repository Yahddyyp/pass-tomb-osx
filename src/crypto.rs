use age::secrecy::SecretString;
use anyhow::{Context, Result};

// Encrypt plaintext with a passphrase (scrypt).
pub fn encrypt_with_passphrase(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let pass = SecretString::from(passphrase.to_owned());
    let recipient = age::scrypt::Recipient::new(pass.clone());

    let encrypted = age::encrypt(&recipient, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    Ok(encrypted)
}

// Decrypt ciphertext with a passphrase (scrypt).
pub fn decrypt_with_passphrase(passphrase: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
    let pass = SecretString::from(passphrase.to_owned());
    let identity = age::scrypt::Identity::new(pass);

    let decrypted = age::decrypt(&identity, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;

    Ok(decrypted)
}

// Encrypt plaintext with an x25519 public key.
pub fn encrypt_with_recipient(
    pubkey: &age::x25519::Recipient,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let encrypted =
        age::encrypt(pubkey, plaintext).map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    Ok(encrypted)
}

// Decrypt ciphertext with an x25519 identity (private key).
pub fn decrypt_with_identity(
    identity: &age::x25519::Identity,
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let decrypted = age::decrypt(identity, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(decrypted)
}

// Generate a new x25519 key pair.
pub fn generate_keypair() -> age::x25519::Identity {
    age::x25519::Identity::generate()
}

// Read an x25519 identity from an age key file (the private key string).
pub fn identity_from_string(s: &str) -> Result<age::x25519::Identity> {
    s.parse()
        .map_err(|e: &'static str| anyhow::anyhow!("failed to parse age identity: {e}"))
}

// Format an identity's public key as a bech32 string.
pub fn public_key_string(identity: &age::x25519::Identity) -> String {
    identity.to_public().to_string()
}

// Encrypt a file at `input_path`, write to `output_path` using a passphrase.
pub fn encrypt_file_with_passphrase(
    passphrase: &str,
    input_path: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<()> {
    let data =
        std::fs::read(input_path).with_context(|| format!("reading {}", input_path.display()))?;
    let encrypted = encrypt_with_passphrase(passphrase, &data)?;
    std::fs::write(output_path, &encrypted)
        .with_context(|| format!("writing {}", output_path.display()))?;
    Ok(())
}

// Decrypt a file at `input_path`, write to `output_path` using a passphrase.
pub fn decrypt_file_with_passphrase(
    passphrase: &str,
    input_path: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<()> {
    let data =
        std::fs::read(input_path).with_context(|| format!("reading {}", input_path.display()))?;
    let decrypted = decrypt_with_passphrase(passphrase, &data)?;
    std::fs::write(output_path, &decrypted)
        .with_context(|| format!("writing {}", output_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passphrase_roundtrip() {
        let data = b"Hello, pass-tomb!";
        let passphrase = "test-passphrase-123";

        let encrypted = encrypt_with_passphrase(passphrase, data).unwrap();
        assert_ne!(encrypted, data);

        let decrypted = decrypt_with_passphrase(passphrase, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let data = b"secret data";
        let encrypted = encrypt_with_passphrase("correct-horse", data).unwrap();
        let result = decrypt_with_passphrase("wrong-horse", &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_x25519_roundtrip() {
        let identity = generate_keypair();
        let pubkey = identity.to_public();
        let data = b"Hello with x25519!";

        let encrypted = encrypt_with_recipient(&pubkey, data).unwrap();
        let decrypted = decrypt_with_identity(&identity, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }
}
