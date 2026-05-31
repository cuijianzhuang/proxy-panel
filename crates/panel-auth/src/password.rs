use argon2::password_hash::{rand_core::OsRng, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};

use crate::error::{Error, Result};

/// Hash a password using argon2id with a random per-password salt.
/// Returns the standard PHC string (encodes algorithm, params, salt, and hash).
pub fn hash_password(plaintext: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    argon
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::PasswordHash(e.to_string()))
}

/// Constant-time verify against a stored PHC hash. Returns `Ok(true)` if the
/// password matches, `Ok(false)` if not, and `Err` if the stored hash is
/// malformed.
pub fn verify_password(plaintext: &str, stored_hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(stored_hash).map_err(|e| Error::PasswordHash(e.to_string()))?;
    match Argon2::default().verify_password(plaintext.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(Error::PasswordHash(e.to_string())),
    }
}
