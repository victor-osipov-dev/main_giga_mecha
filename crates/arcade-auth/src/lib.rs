//! Minecraft online-mode authentication.
//!
//! The server proves the client owns the account by asking Mojang's session
//! server about the SHA-1 hash of the shared secret and our public key.

use arcade_protocol::{GameProfile, ProfileProperty};
use num_bigint::BigInt;
use rand_core::OsRng;
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};
use sha1::{Digest, Sha1};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("could not build the RSA key: {0}")]
    Key(#[from] rsa::Error),
    #[error("could not encode the public key: {0}")]
    PublicKey(#[from] rsa::pkcs8::spki::Error),
    #[error("could not decrypt the client's response")]
    Decrypt,
    #[error("the client did not prove knowledge of the verification token")]
    VerifyToken,
    #[error("session server request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("session server returned an invalid response: {0}")]
    InvalidSession(String),
}

/// The 1024-bit RSA key pair Minecraft uses during login.
pub struct EncryptionKeys {
    pub private: RsaPrivateKey,
    /// DER-encoded SubjectPublicKeyInfo.
    pub public_der: Vec<u8>,
}

impl EncryptionKeys {
    pub fn generate() -> Result<Self, AuthError> {
        let mut rng = OsRng;
        let private = RsaPrivateKey::new(&mut rng, 1024)?;
        let public_der = private
            .to_public_key()
            .to_public_key_der()?
            .as_bytes()
            .to_vec();
        Ok(Self {
            private,
            public_der,
        })
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, AuthError> {
        self.private
            .decrypt(Pkcs1v15Encrypt, data)
            .map_err(|_| AuthError::Decrypt)
    }
}

/// Minecraft's signed-hex SHA-1 of the shared secret and public key.
pub fn server_hash(secret: &[u8], public_key: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(secret);
    hasher.update(public_key);
    let digest = hasher.finalize();
    BigInt::from_signed_bytes_be(&digest).to_str_radix(16)
}

#[derive(serde::Deserialize)]
struct SessionProfile {
    id: String,
    name: String,
    #[serde(default)]
    properties: Vec<ProfileProperty>,
}

/// Client used for session-server calls, with a bounded timeout so a broken
/// Mojang service cannot pin login tasks forever.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("VoxelArcade/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("static reqwest configuration is valid")
}

/// Asks Mojang whether `name` completed the join for `hash`.
///
/// `Ok(None)` means the session is not valid (wrong account, expired, or a
/// spoofed login). Transport errors are reported separately.
pub async fn has_joined(
    http: &reqwest::Client,
    username: &str,
    hash: &str,
    ip: &str,
) -> Result<Option<GameProfile>, AuthError> {
    let mut url = format!(
        "https://sessionserver.mojang.com/session/minecraft/hasJoined?username={}&serverId={}",
        urlencode(username),
        urlencode(hash)
    );
    if let Ok(parsed) = ip.parse::<std::net::IpAddr>() {
        if !parsed.is_loopback() {
            url.push_str("&ip=");
            url.push_str(&parsed.to_string());
        }
    }
    let response = http.get(url).send().await?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    let session: SessionProfile = response.json().await?;
    let id = Uuid::parse_str(&session.id)
        .map_err(|e| AuthError::InvalidSession(format!("bad uuid: {e}")))?;
    Ok(Some(GameProfile {
        id,
        name: session.name,
        properties: session.properties,
    }))
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_vanilla_shape() {
        // Known-answer vector: SHA-1("Notch") as a signed big-endian integer.
        let h = server_hash(b"Notch", b"");
        assert_eq!(h, "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        let h = server_hash(b"jeb_", b"");
        assert_eq!(h, "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
    }

    #[test]
    fn rsa_roundtrip() {
        let keys = EncryptionKeys::generate().unwrap();
        let mut rng = OsRng;
        let msg = b"shared-secret";
        let enc = keys
            .private
            .to_public_key()
            .encrypt(&mut rng, Pkcs1v15Encrypt, msg)
            .unwrap();
        assert_eq!(keys.decrypt(&enc).unwrap(), msg);
    }
}
