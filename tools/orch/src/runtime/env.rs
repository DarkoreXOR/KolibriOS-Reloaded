//! Environment variable helpers (affect subsequent child processes).

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};

/// Script-local environment overlay applied to child processes.
#[derive(Debug, Default, Clone)]
pub struct EnvOverlay {
    inner: Arc<Mutex<HashMap<String, Option<String>>>>,
}

impl EnvOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let guard = self.inner.lock().unwrap();
        if let Some(v) = guard.get(key) {
            return v.clone();
        }
        env::var(key).ok()
    }

    pub fn set(&self, key: impl Into<String>, value: impl Into<String>) {
        self.inner
            .lock()
            .unwrap()
            .insert(key.into(), Some(value.into()));
    }

    pub fn remove(&self, key: &str) {
        self.inner.lock().unwrap().insert(key.to_string(), None);
    }

    pub fn enumerate(&self) -> HashMap<String, String> {
        let mut map: HashMap<String, String> = env::vars().collect();
        let guard = self.inner.lock().unwrap();
        for (k, v) in guard.iter() {
            match v {
                Some(val) => {
                    map.insert(k.clone(), val.clone());
                }
                None => {
                    map.remove(k);
                }
            }
        }
        map
    }

    pub fn apply_to(&self, cmd: &mut std::process::Command) {
        let guard = self.inner.lock().unwrap();
        for (k, v) in guard.iter() {
            match v {
                Some(val) => {
                    cmd.env(k, val);
                }
                None => {
                    cmd.env_remove(k);
                }
            }
        }
    }
}
