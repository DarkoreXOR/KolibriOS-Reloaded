//! First-class pipe primitives (no shell parser).

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// In-memory byte pipe connecting producers and consumers.
#[derive(Clone)]
pub struct BytePipe {
    tx: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
    rx: Arc<Mutex<Option<Receiver<Vec<u8>>>>>,
}

impl BytePipe {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx: Arc::new(Mutex::new(Some(tx))),
            rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    pub fn write_bytes(&self, data: Vec<u8>) -> Result<(), String> {
        let guard = self.tx.lock().unwrap();
        let tx = guard.as_ref().ok_or_else(|| "pipe closed".to_string())?;
        tx.send(data).map_err(|e| e.to_string())
    }

    pub fn read_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        let guard = self.rx.lock().unwrap();
        let rx = guard.as_ref().ok_or_else(|| "pipe closed".to_string())?;
        match rx.recv() {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    pub fn close(&self) {
        *self.tx.lock().unwrap() = None;
    }
}

impl Default for BytePipe {
    fn default() -> Self {
        Self::new()
    }
}

/// Copy all bytes from `reader` into the pipe (background).
pub fn pump_reader_to_pipe<R: Read + Send + 'static>(mut reader: R, pipe: BytePipe) {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if pipe.write_bytes(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        pipe.close();
    });
}

/// Drain pipe into `writer` (background).
pub fn pump_pipe_to_writer<W: Write + Send + 'static>(pipe: BytePipe, mut writer: W) {
    thread::spawn(move || {
        while let Ok(Some(chunk)) = pipe.read_bytes() {
            if writer.write_all(&chunk).is_err() {
                break;
            }
        }
        let _ = writer.flush();
    });
}
