//! Cancellation-aware TCP/UDP networking primitives.

use super::cancel::{CancelToken, Cancelled};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct TcpConn {
    stream: Arc<Mutex<TcpStream>>,
}

impl TcpConn {
    pub fn connect(addr: &str, cancel: &CancelToken, poll_ms: u64) -> Result<Self, NetError> {
        let addrs: Vec<_> = addr
            .to_socket_addrs()
            .map_err(NetError::Io)?
            .collect();
        if addrs.is_empty() {
            return Err(NetError::Msg(format!("could not resolve {addr}")));
        }
        loop {
            cancel.throw_if_requested()?;
            match TcpStream::connect_timeout(&addrs[0], Duration::from_millis(poll_ms.max(1))) {
                Ok(stream) => {
                    stream
                        .set_read_timeout(Some(Duration::from_millis(poll_ms.max(1))))
                        .ok();
                    stream
                        .set_write_timeout(Some(Duration::from_millis(poll_ms.max(1))))
                        .ok();
                    return Ok(Self {
                        stream: Arc::new(Mutex::new(stream)),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(NetError::Io(e)),
            }
        }
    }

    pub fn read(&self, max: i64, cancel: &CancelToken) -> Result<Vec<u8>, NetError> {
        let mut buf = vec![0u8; max.max(0) as usize];
        loop {
            cancel.throw_if_requested()?;
            let mut g = self.stream.lock().unwrap();
            match g.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    return Ok(buf);
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    drop(g);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(NetError::Io(e)),
            }
        }
    }

    pub fn write(&self, data: &[u8], cancel: &CancelToken) -> Result<(), NetError> {
        cancel.throw_if_requested()?;
        let mut g = self.stream.lock().unwrap();
        g.write_all(data).map_err(NetError::Io)
    }

    pub fn close(&self) -> Result<(), NetError> {
        let g = self.stream.lock().unwrap();
        g.shutdown(std::net::Shutdown::Both).map_err(NetError::Io)
    }
}

#[derive(Clone)]
pub struct TcpServer {
    listener: Arc<Mutex<TcpListener>>,
}

impl TcpServer {
    pub fn listen(addr: &str) -> Result<Self, NetError> {
        let listener = TcpListener::bind(addr).map_err(NetError::Io)?;
        listener.set_nonblocking(true).map_err(NetError::Io)?;
        Ok(Self {
            listener: Arc::new(Mutex::new(listener)),
        })
    }

    pub fn accept(&self, cancel: &CancelToken, poll_ms: u64) -> Result<TcpConn, NetError> {
        loop {
            cancel.throw_if_requested()?;
            let listener = self.listener.lock().unwrap();
            match listener.accept() {
                Ok((stream, _)) => {
                    drop(listener);
                    stream
                        .set_read_timeout(Some(Duration::from_millis(poll_ms.max(1))))
                        .ok();
                    return Ok(TcpConn {
                        stream: Arc::new(Mutex::new(stream)),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    drop(listener);
                    thread::sleep(Duration::from_millis(poll_ms.max(1)));
                }
                Err(e) => return Err(NetError::Io(e)),
            }
        }
    }
}

#[derive(Clone)]
pub struct UdpSock {
    sock: Arc<Mutex<UdpSocket>>,
}

impl UdpSock {
    pub fn bind(addr: &str) -> Result<Self, NetError> {
        let sock = UdpSocket::bind(addr).map_err(NetError::Io)?;
        sock.set_read_timeout(Some(Duration::from_millis(50))).ok();
        Ok(Self {
            sock: Arc::new(Mutex::new(sock)),
        })
    }

    pub fn send_to(&self, data: &[u8], addr: &str) -> Result<usize, NetError> {
        self.sock
            .lock()
            .unwrap()
            .send_to(data, addr)
            .map_err(NetError::Io)
    }

    pub fn recv(&self, max: i64, cancel: &CancelToken) -> Result<(Vec<u8>, String), NetError> {
        let mut buf = vec![0u8; max.max(0) as usize];
        loop {
            cancel.throw_if_requested()?;
            match self.sock.lock().unwrap().recv_from(&mut buf) {
                Ok((n, addr)) => {
                    buf.truncate(n);
                    return Ok((buf, addr.to_string()));
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(NetError::Io(e)),
            }
        }
    }
}

#[derive(Debug)]
pub enum NetError {
    Cancelled,
    Io(std::io::Error),
    Msg(String),
}

impl From<Cancelled> for NetError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancellation requested"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Msg(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for NetError {}
