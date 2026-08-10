//! OS-backed RNG helpers.

use getrandom::getrandom;
use rand::Rng;
use uuid::Uuid;

pub fn random_bytes(n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    getrandom(&mut buf).map_err(|e| format!("secure RNG failed: {e}"))?;
    Ok(buf)
}

pub fn random_u64_bounded(max_exclusive: u64) -> Result<u64, String> {
    if max_exclusive == 0 {
        return Err("random bound must be > 0".into());
    }
    Ok(rand::thread_rng().gen_range(0..max_exclusive))
}

pub fn random_uuid_v4() -> String {
    Uuid::new_v4().to_string()
}
