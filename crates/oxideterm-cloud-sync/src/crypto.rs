// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! End-to-end payload encryption for cloud-sync snapshots.
//!
//! The signaling worker and any peer without the room passphrase must never
//! read snapshot contents. A key is derived from the room passphrase with
//! Argon2id (fixed domain-separation salt so every peer in the room derives
//! the same key) and every snapshot is sealed with ChaCha20-Poly1305 using a
//! fresh random nonce. The sealed envelope travels inside `SyncEnvelope.data`,
//! so the transport layer stays format-agnostic.
//!
//! The passphrase doubles as the room secret; deriving the key from it keeps
//! key management implicit while the KDF cost slows down offline guessing of
//! weak passphrases.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Domain-separation salt for the Argon2id derivation. Not secret: its only
/// job is to keep cloud-sync keys distinct from any other use of the same
/// passphrase and to make the derivation deterministic across peers.
const KDF_SALT: &[u8] = b"oxideterm-cloud-sync-v1";
/// Argon2id cost tuned for a background sync engine: strong enough against
/// offline brute force of typical passphrases without blocking the UI thread
/// or ballooning memory on small machines.
const KDF_MEMORY_KIB: u32 = 64 * 1024;
const KDF_ITERATIONS: u32 = 3;
const KDF_PARALLELISM: u32 = 1;
const NONCE_LEN: usize = 12;

const ENVELOPE_VERSION: u32 = 1;
const ENVELOPE_ALGORITHM: &str = "chacha20poly1305";

/// One sealed snapshot. `ciphertext` is base64 and `nonce` is hex so the
/// envelope serializes cleanly as a plain JSON string payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedSyncPayload {
    pub v: u32,
    pub alg: String,
    #[serde(rename = "nonceHex")]
    pub nonce_hex: String,
    #[serde(rename = "ctBase64")]
    pub ct_base64: String,
}

/// Derives the shared 32-byte snapshot key from the room passphrase.
pub fn derive_snapshot_key(room_passphrase: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_PARALLELISM, Some(32))
        .map_err(|error| format!("invalid Argon2id parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(room_passphrase.trim().as_bytes(), KDF_SALT, &mut *key)
        .map_err(|error| format!("failed to derive snapshot key: {error}"))?;
    Ok(key)
}

/// Encrypts a plaintext snapshot document. The caller owns the key and may
/// cache it for the lifetime of the sync session.
pub fn encrypt_snapshot(
    key: &Zeroizing<[u8; 32]>,
    plaintext: &str,
) -> Result<EncryptedSyncPayload, String> {
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| "invalid snapshot key length".to_string())?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| "snapshot encryption failed".to_string())?;
    use base64::Engine as _;
    Ok(EncryptedSyncPayload {
        v: ENVELOPE_VERSION,
        alg: ENVELOPE_ALGORITHM.to_string(),
        nonce_hex: hex(&nonce),
        ct_base64: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

/// Decrypts a sealed snapshot envelope. Fails on any tampering (bad tag,
/// unknown algorithm/version) so corrupted or hostile frames never reach the
/// merge layer.
pub fn decrypt_snapshot(
    key: &Zeroizing<[u8; 32]>,
    envelope: &EncryptedSyncPayload,
) -> Result<Zeroizing<String>, String> {
    if envelope.v != ENVELOPE_VERSION || envelope.alg != ENVELOPE_ALGORITHM {
        return Err("unsupported snapshot envelope".to_string());
    }
    let nonce = unhex(&envelope.nonce_hex).ok_or_else(|| "invalid snapshot nonce".to_string())?;
    if nonce.len() != NONCE_LEN {
        return Err("invalid snapshot nonce length".to_string());
    }
    use base64::Engine as _;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&envelope.ct_base64)
        .map_err(|_| "invalid snapshot ciphertext".to_string())?;
    let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| "invalid snapshot key length".to_string())?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "snapshot authentication failed".to_string())?;
    Ok(Zeroizing::new(
        String::from_utf8(plaintext).map_err(|_| "snapshot is not valid UTF-8".to_string())?,
    ))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for chunk in bytes.chunks_exact(2) {
        let high = (chunk[0] as char).to_digit(16)? as u8;
        let low = (chunk[1] as char).to_digit(16)? as u8;
        out.push((high << 4) | low);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn roundtrip_encrypts_and_decrypts_snapshot() {
        let key = derive_snapshot_key("correct horse battery staple").expect("derive key");
        let sealed = encrypt_snapshot(&key, r#"{"connections":[]}"#).expect("encrypt");
        let opened = decrypt_snapshot(&key, &sealed).expect("decrypt");
        assert_eq!(opened.as_str(), r#"{"connections":[]}"#);
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let key = derive_snapshot_key("room-secret").expect("derive key");
        let mut sealed = encrypt_snapshot(&key, r#"{"connections":[]}"#).expect("encrypt");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&sealed.ct_base64)
            .expect("decode");
        let mut tampered = bytes.clone();
        if let Some(last) = tampered.last_mut() {
            *last ^= 0x01;
        }
        sealed.ct_base64 = base64::engine::general_purpose::STANDARD.encode(tampered);
        assert!(decrypt_snapshot(&key, &sealed).is_err());
    }

    #[test]
    fn peers_with_same_passphrase_derive_same_key() {
        let left = derive_snapshot_key("shared room").expect("left");
        let right = derive_snapshot_key("shared room").expect("right");
        assert_eq!(left.as_ref(), right.as_ref());
    }
}
