//! Encoding helpers (hex / Base64).

use base64::{engine::general_purpose::STANDARD as B64, Engine};

pub fn hex_encode(data: &[u8]) -> String {
    hex::encode(data)
}

pub fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    hex::decode(s).map_err(|e| e.to_string())
}

pub fn base64_encode(data: &[u8]) -> String {
    B64.encode(data)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s).map_err(|e| e.to_string())
}
