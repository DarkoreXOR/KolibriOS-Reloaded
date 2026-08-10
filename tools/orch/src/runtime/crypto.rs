//! Cryptographic hashing and HMAC helpers.
//!
//! SHA-1 and MD5 are provided for legacy/compatibility only — not for security.

use hmac::{Hmac, Mac};
use md5::{Digest as Md5Digest, Md5};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256, Sha512};

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    Sha2Digest::update(&mut h, data);
    hex::encode(h.finalize())
}

pub fn sha512_hex(data: &[u8]) -> String {
    let mut h = Sha512::new();
    Sha2Digest::update(&mut h, data);
    hex::encode(h.finalize())
}

/// Legacy / compatibility hash — not cryptographically secure.
pub fn sha1_hex(data: &[u8]) -> String {
    let mut h = Sha1::new();
    Sha1Digest::update(&mut h, data);
    hex::encode(h.finalize())
}

/// Legacy / compatibility hash — not cryptographically secure.
pub fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    Md5Digest::update(&mut h, data);
    hex::encode(h.finalize())
}

pub fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|e| e.to_string())?;
    mac.update(data);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn hmac_sha512_hex(key: &[u8], data: &[u8]) -> Result<String, String> {
    let mut mac = HmacSha512::new_from_slice(key).map_err(|e| e.to_string())?;
    mac.update(data);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
