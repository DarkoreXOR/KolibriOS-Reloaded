//! Register generic native modules on a Rhai Engine.

use crate::execution::events::{EventSink, ExecutionEvent};
use crate::execution::rollback::{CleanupStack, RollbackStack};
use crate::execution::tree::ExecId;
use crate::runtime::cancel::CancelToken;
use crate::runtime::env::EnvOverlay;
use crate::runtime::path::ScriptCwd;
use crate::runtime::process::{IoMode, ProcessHandle, ProcessSpec, ProcessTracker};
use crate::runtime::{crypto, encoding, fs as rfs, http, pipe, rng, socket, string as rstring, timer, toml_util};
use rhai::{Dynamic, Engine, EvalAltResult, Map};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RuntimeHandles {
    pub cwd: ScriptCwd,
    pub env: EnvOverlay,
    pub cancel: CancelToken,
    pub sink: Arc<Mutex<dyn EventSink>>,
    pub exec_id: ExecId,
    pub depth: usize,
    pub rollback: Arc<Mutex<RollbackStack>>,
    pub cleanup: Arc<Mutex<CleanupStack>>,
    pub poll_ms: u64,
    pub termination_timeout_ms: u64,
    pub processes: ProcessTracker,
    pub args: Vec<String>,
    /// Unit args as map (flags / key-values).
    pub unit_args: Map,
}

pub fn register_runtime_modules(engine: &mut Engine, handles: RuntimeHandles) {
    let h = handles.clone();

    // --- args ---
    engine.register_fn("get_args", {
        let args = h.args.clone();
        move || -> Dynamic {
            let arr: rhai::Array = args.iter().map(|s| Dynamic::from(s.clone())).collect();
            Dynamic::from(arr)
        }
    });

    // --- fail / cancel ---
    engine.register_fn("fail", |msg: &str| -> Result<(), Box<EvalAltResult>> {
        Err(format!("fail: {msg}").into())
    });

    engine.register_fn("cancel__requested", {
        let c = h.cancel.clone();
        move || c.requested()
    });
    engine.register_fn("cancel__request", {
        let c = h.cancel.clone();
        move || c.request()
    });
    engine.register_fn("cancel__throw_if_requested", {
        let c = h.cancel.clone();
        move || -> Result<(), Box<EvalAltResult>> {
            c.throw_if_requested()
                .map_err(|e| e.to_string().into())
        }
    });

    // --- log ---
    {
        let sink = Arc::clone(&h.sink);
        let id = h.exec_id.0.to_string();
        let depth = h.depth;
        engine.register_fn("log__info", {
            let sink = Arc::clone(&sink);
            let id = id.clone();
            move |msg: &str| {
                sink.lock().unwrap().emit(ExecutionEvent::LogInfo {
                    id: id.clone(),
                    message: msg.into(),
                    depth,
                });
            }
        });
        engine.register_fn("log__warn", {
            let sink = Arc::clone(&sink);
            let id = id.clone();
            move |msg: &str| {
                sink.lock().unwrap().emit(ExecutionEvent::LogWarn {
                    id: id.clone(),
                    message: msg.into(),
                    depth,
                });
            }
        });
        engine.register_fn("log__error", {
            let sink = Arc::clone(&sink);
            let id = id.clone();
            move |msg: &str| {
                sink.lock().unwrap().emit(ExecutionEvent::LogError {
                    id: id.clone(),
                    message: msg.into(),
                    depth,
                });
            }
        });
    }

    // --- path ---
    engine.register_fn("path__cwd", {
        let cwd = h.cwd.clone();
        move || cwd.get().display().to_string()
    });
    engine.register_fn("path__chdir", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<String, Box<EvalAltResult>> {
            cwd.chdir(p)
                .map(|p| p.display().to_string())
                .map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("path__resolve", {
        let cwd = h.cwd.clone();
        move |p: &str| cwd.resolve(p).display().to_string()
    });
    engine.register_fn("path__which", |name: &str| -> Dynamic {
        which_on_path(name)
            .map(|p| Dynamic::from(p.display().to_string()))
            .unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("path__is_file", {
        let cwd = h.cwd.clone();
        move |p: &str| cwd.resolve(p).is_file()
    });
    engine.register_fn("path__is_dir", {
        let cwd = h.cwd.clone();
        move |p: &str| cwd.resolve(p).is_dir()
    });
    engine.register_fn("path__is_absolute", |p: &str| {
        std::path::Path::new(p).is_absolute()
    });
    engine.register_fn("path__join", {
        let cwd = h.cwd.clone();
        move |parts: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let list = dynamic_to_string_vec(parts)?;
            Ok(rfs::path_join(&cwd, &list))
        }
    });
    engine.register_fn("path__temp_dir", || rfs::temp_dir().display().to_string());

    // --- time ---
    engine.register_fn("time__utc_stamp", || {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Simple UTC YYYYMMDD-HHMMSS without external deps.
        const SECS_PER_DAY: u64 = 86_400;
        let days = secs / SECS_PER_DAY;
        let tod = secs % SECS_PER_DAY;
        let hour = tod / 3600;
        let min = (tod % 3600) / 60;
        let sec = tod % 60;
        let (y, m, d) = civil_from_days(days as i64);
        format!("{y:04}{m:02}{d:02}-{hour:02}{min:02}{sec:02}")
    });

    // --- toml ---
    engine.register_fn("toml__load", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            toml_util::load(&cwd.resolve(p)).map_err(|e| e.into())
        }
    });

    // --- string extensions (Rhai builtins already cover starts_with/ends_with/…) ---
    rstring::register(engine);

    // --- env ---
    engine.register_fn("env__get", {
        let env = h.env.clone();
        move |k: &str| env.get(k).map(Dynamic::from).unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("env__set", {
        let env = h.env.clone();
        move |k: &str, v: &str| env.set(k, v)
    });
    engine.register_fn("env__remove", {
        let env = h.env.clone();
        move |k: &str| env.remove(k)
    });

    // --- timer ---
    engine.register_fn("timer__sleep", {
        let c = h.cancel.clone();
        let poll = h.poll_ms;
        move |ms: i64| -> Result<(), Box<EvalAltResult>> {
            timer::sleep_ms(ms.max(0) as u64, &c, poll).map_err(|e| e.to_string().into())
        }
    });

    // --- filesystem ---
    engine.register_fn("file__exists", {
        let cwd = h.cwd.clone();
        move |p: &str| rfs::exists(&cwd, p)
    });
    engine.register_fn("file__read_all", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<String, Box<EvalAltResult>> {
            let bytes = rfs::read_all(&cwd, p).map_err(|e| e.to_string())?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    });
    engine.register_fn("file__write_all", {
        let cwd = h.cwd.clone();
        move |p: &str, data: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::write_all(&cwd, p, data.as_bytes()).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("file__append", {
        let cwd = h.cwd.clone();
        move |p: &str, data: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::append(&cwd, p, data.as_bytes()).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("file__metadata", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<Map, Box<EvalAltResult>> {
            let m = rfs::metadata(&cwd, p).map_err(|e| e.to_string())?;
            let mut map = Map::new();
            map.insert("path".into(), Dynamic::from(m.path));
            map.insert("len".into(), Dynamic::from(m.len as i64));
            map.insert("is_file".into(), Dynamic::from(m.is_file));
            map.insert("is_dir".into(), Dynamic::from(m.is_dir));
            map.insert("readonly".into(), Dynamic::from(m.readonly));
            Ok(map)
        }
    });
    engine.register_fn("file__copy", {
        let cwd = h.cwd.clone();
        move |a: &str, b: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::copy(&cwd, a, b).map(|_| ()).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("file__move", {
        let cwd = h.cwd.clone();
        move |a: &str, b: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::rename(&cwd, a, b).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("file__remove", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::remove_path(&cwd, p).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("dir__create", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::dir_create(&cwd, p).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("dir__remove", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<(), Box<EvalAltResult>> {
            rfs::dir_remove(&cwd, p).map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("dir__exists", {
        let cwd = h.cwd.clone();
        move |p: &str| rfs::dir_exists(&cwd, p)
    });
    engine.register_fn("dir__list", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            let names = rfs::dir_list(&cwd, p).map_err(|e| e.to_string())?;
            let arr: rhai::Array = names.into_iter().map(Dynamic::from).collect();
            Ok(Dynamic::from(arr))
        }
    });
    engine.register_fn("dir__walk", {
        let cwd = h.cwd.clone();
        move |p: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            let names = rfs::walk(&cwd, p).map_err(|e| e.to_string())?;
            let arr: rhai::Array = names.into_iter().map(Dynamic::from).collect();
            Ok(Dynamic::from(arr))
        }
    });

    // --- process ---
    engine
        .register_type_with_name::<ProcessHandle>("Process")
        .register_fn("poll_running", |p: &mut ProcessHandle| {
            matches!(p.poll().ok(), Some(crate::runtime::process::PollResult::Running))
        })
        .register_fn("wait", |p: &mut ProcessHandle| -> Result<i64, Box<EvalAltResult>> {
            let status = p.wait().map_err(|e| e.to_string())?;
            Ok(status.code().unwrap_or(-1) as i64)
        })
        .register_fn("kill", |p: &mut ProcessHandle| -> Result<(), Box<EvalAltResult>> {
            p.kill().map_err(|e| e.to_string().into())
        })
        .register_fn("stdout_text", |p: &mut ProcessHandle| p.stdout_captured())
        .register_fn("stderr_text", |p: &mut ProcessHandle| p.stderr_captured())
        .register_fn("success", |code: i64| code == 0);

    engine.register_fn("process__run", {
        let cwd = h.cwd.clone();
        let env = h.env.clone();
        let cancel = h.cancel.clone();
        let poll = h.poll_ms;
        let term = h.termination_timeout_ms;
        let tracker = h.processes.clone();
        let sink = Arc::clone(&h.sink);
        let parent = h.exec_id.0.to_string();
        let depth = h.depth + 1;
        move |program: &str, args: Dynamic| -> Result<ProcessHandle, Box<EvalAltResult>> {
            let arg_list = dynamic_to_string_vec(args)?;
            let mut spec = ProcessSpec::new(program, arg_list);
            spec.stdout = IoMode::Inherit;
            spec.stderr = IoMode::Inherit;
            let handle =
                ProcessHandle::spawn(spec, &cwd, &env, cancel.clone(), poll, tracker.clone(), term)
                    .map_err(|e| e.to_string())?;
            sink.lock().unwrap().emit(ExecutionEvent::ProcessStarted {
                id: uuid::Uuid::new_v4().to_string(),
                parent: Some(parent.clone()),
                program: program.into(),
                depth,
            });
            Ok(handle)
        }
    });

    engine.register_fn("process__run_capture", {
        let cwd = h.cwd.clone();
        let env = h.env.clone();
        let cancel = h.cancel.clone();
        let poll = h.poll_ms;
        let term = h.termination_timeout_ms;
        let tracker = h.processes.clone();
        move |program: &str, args: Dynamic| -> Result<ProcessHandle, Box<EvalAltResult>> {
            let arg_list = dynamic_to_string_vec(args)?;
            let mut spec = ProcessSpec::new(program, arg_list);
            spec.stdout = IoMode::Capture;
            spec.stderr = IoMode::Capture;
            ProcessHandle::spawn(spec, &cwd, &env, cancel.clone(), poll, tracker.clone(), term)
                .map_err(|e| e.to_string().into())
        }
    });

    engine.register_fn("process__run_cwd", {
        let cwd = h.cwd.clone();
        let env = h.env.clone();
        let cancel = h.cancel.clone();
        let poll = h.poll_ms;
        let term = h.termination_timeout_ms;
        let tracker = h.processes.clone();
        move |program: &str, args: Dynamic, child_cwd: &str| -> Result<ProcessHandle, Box<EvalAltResult>> {
            let arg_list = dynamic_to_string_vec(args)?;
            let mut spec = ProcessSpec::new(program, arg_list);
            spec.cwd = Some(cwd.resolve(child_cwd));
            ProcessHandle::spawn(spec, &cwd, &env, cancel.clone(), poll, tracker.clone(), term)
                .map_err(|e| e.to_string().into())
        }
    });

    engine.register_fn("process__write_stdin", {
        move |p: &mut ProcessHandle, data: &str| -> Result<(), Box<EvalAltResult>> {
            p.write_stdin(data.as_bytes())
                .map_err(|e| e.to_string().into())
        }
    });
    engine.register_fn("process__close_stdin", {
        move |p: &mut ProcessHandle| -> Result<(), Box<EvalAltResult>> {
            p.close_stdin().map_err(|e| e.to_string().into())
        }
    });

    // Feed stdin text, capture stdout/stderr. Closes stdin after write.
    engine.register_fn("process__run_stdin", {
        let cwd = h.cwd.clone();
        let env = h.env.clone();
        let cancel = h.cancel.clone();
        let poll = h.poll_ms;
        let term = h.termination_timeout_ms;
        let tracker = h.processes.clone();
        move |program: &str, args: Dynamic, stdin_text: &str| -> Result<ProcessHandle, Box<EvalAltResult>> {
            let arg_list = dynamic_to_string_vec(args)?;
            let mut spec = ProcessSpec::new(program, arg_list);
            spec.stdin = IoMode::Pipe;
            spec.stdout = IoMode::Capture;
            spec.stderr = IoMode::Capture;
            let handle =
                ProcessHandle::spawn(spec, &cwd, &env, cancel.clone(), poll, tracker.clone(), term)
                    .map_err(|e| e.to_string())?;
            handle
                .write_stdin(stdin_text.as_bytes())
                .map_err(|e| e.to_string())?;
            handle.close_stdin().map_err(|e| e.to_string())?;
            Ok(handle)
        }
    });

    // A | B style: capture A's stdout, feed to B's stdin. Returns B's exit code.
    // Intentionally buffered (not a live OS pipe) for portability and cancel safety.
    engine.register_fn("process__pipeline", {
        let cwd = h.cwd.clone();
        let env = h.env.clone();
        let cancel = h.cancel.clone();
        let poll = h.poll_ms;
        let term = h.termination_timeout_ms;
        let tracker = h.processes.clone();
        move |prog_a: &str,
              args_a: Dynamic,
              prog_b: &str,
              args_b: Dynamic|
              -> Result<i64, Box<EvalAltResult>> {
            let list_a = dynamic_to_string_vec(args_a)?;
            let list_b = dynamic_to_string_vec(args_b)?;
            let mut spec_a = ProcessSpec::new(prog_a, list_a);
            spec_a.stdout = IoMode::Capture;
            spec_a.stderr = IoMode::Capture;
            let a = ProcessHandle::spawn(
                spec_a,
                &cwd,
                &env,
                cancel.clone(),
                poll,
                tracker.clone(),
                term,
            )
            .map_err(|e| e.to_string())?;
            let code_a = a.wait().map_err(|e| e.to_string())?;
            if !code_a.success() {
                return Err(format!(
                    "pipeline left side failed: {} exit {}",
                    prog_a,
                    code_a.code().unwrap_or(-1)
                )
                .into());
            }
            let out = a.stdout_captured();
            let mut spec_b = ProcessSpec::new(prog_b, list_b);
            spec_b.stdin = IoMode::Pipe;
            spec_b.stdout = IoMode::Capture;
            spec_b.stderr = IoMode::Capture;
            let b = ProcessHandle::spawn(
                spec_b,
                &cwd,
                &env,
                cancel.clone(),
                poll,
                tracker.clone(),
                term,
            )
            .map_err(|e| e.to_string())?;
            b.write_stdin(out.as_bytes()).map_err(|e| e.to_string())?;
            b.close_stdin().map_err(|e| e.to_string())?;
            let code_b = b.wait().map_err(|e| e.to_string())?;
            Ok(code_b.code().unwrap_or(-1) as i64)
        }
    });

    // --- pipes ---
    engine
        .register_type_with_name::<pipe::BytePipe>("Pipe")
        .register_fn("pipe__new", pipe::BytePipe::new)
        .register_fn("write_bytes", |p: &mut pipe::BytePipe, s: &str| -> Result<(), Box<EvalAltResult>> {
            p.write_bytes(s.as_bytes().to_vec()).map_err(|e| e.into())
        })
        .register_fn("read_text", |p: &mut pipe::BytePipe| -> Result<String, Box<EvalAltResult>> {
            match p.read_bytes().map_err(|e| e.to_string())? {
                Some(b) => Ok(String::from_utf8_lossy(&b).into_owned()),
                None => Ok(String::new()),
            }
        })
        .register_fn("close", |p: &mut pipe::BytePipe| p.close());

    // --- sockets ---
    engine.register_fn("tcp__connect", {
        let c = h.cancel.clone();
        let poll = h.poll_ms;
        move |addr: &str| -> Result<socket::TcpConn, Box<EvalAltResult>> {
            socket::TcpConn::connect(addr, &c, poll).map_err(|e| e.to_string().into())
        }
    });
    engine
        .register_type_with_name::<socket::TcpConn>("TcpConn")
        .register_fn("read_text", {
            let c = h.cancel.clone();
            move |conn: &mut socket::TcpConn, max: i64| -> Result<String, Box<EvalAltResult>> {
                let bytes = conn.read(max, &c).map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
        })
        .register_fn("write_text", {
            let c = h.cancel.clone();
            move |conn: &mut socket::TcpConn, data: &str| -> Result<(), Box<EvalAltResult>> {
                conn.write(data.as_bytes(), &c).map_err(|e| e.to_string().into())
            }
        })
        .register_fn("close", |conn: &mut socket::TcpConn| -> Result<(), Box<EvalAltResult>> {
            conn.close().map_err(|e| e.to_string().into())
        });

    engine.register_fn("tcp__listen", |addr: &str| -> Result<socket::TcpServer, Box<EvalAltResult>> {
        socket::TcpServer::listen(addr).map_err(|e| e.to_string().into())
    });
    engine
        .register_type_with_name::<socket::TcpServer>("TcpServer")
        .register_fn("accept", {
            let c = h.cancel.clone();
            let poll = h.poll_ms;
            move |srv: &mut socket::TcpServer| -> Result<socket::TcpConn, Box<EvalAltResult>> {
                srv.accept(&c, poll).map_err(|e| e.to_string().into())
            }
        });

    engine.register_fn("udp__bind", |addr: &str| -> Result<socket::UdpSock, Box<EvalAltResult>> {
        socket::UdpSock::bind(addr).map_err(|e| e.to_string().into())
    });
    engine
        .register_type_with_name::<socket::UdpSock>("UdpSock")
        .register_fn("send_to", |s: &mut socket::UdpSock, data: &str, addr: &str| -> Result<i64, Box<EvalAltResult>> {
            s.send_to(data.as_bytes(), addr)
                .map(|n| n as i64)
                .map_err(|e| e.to_string().into())
        })
        .register_fn("recv_text", {
            let c = h.cancel.clone();
            move |s: &mut socket::UdpSock, max: i64| -> Result<String, Box<EvalAltResult>> {
                let (bytes, _addr) = s.recv(max, &c).map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
        });

    // --- execution rollback / cleanup (string bodies; evaluated later with runtime modules) ---
    engine.register_fn("execution__on_rollback", {
        let rb = Arc::clone(&h.rollback);
        let handles = h.clone();
        move |body: &str| -> Result<(), Box<EvalAltResult>> {
            let body = body.to_string();
            let handles = handles.clone();
            rb.lock()
                .unwrap()
                .on_rollback(Box::new(move || {
                    let mut eng = Engine::new();
                    register_runtime_modules(&mut eng, handles.clone());
                    let src = rewrite_module_calls(&body);
                    eng.run(&src).map(|_| ()).map_err(|e| e.to_string())
                }))
                .map_err(|e| e.into())
        }
    });

    engine.register_fn("execution__on_cleanup", {
        let cu = Arc::clone(&h.cleanup);
        let handles = h.clone();
        move |body: &str| {
            let body = body.to_string();
            let handles = handles.clone();
            cu.lock().unwrap().on_cleanup(Box::new(move || {
                let mut eng = Engine::new();
                register_runtime_modules(&mut eng, handles.clone());
                let src = rewrite_module_calls(&body);
                let _ = eng.run(&src);
            }));
        }
    });

    // --- crypto ---
    engine.register_fn("crypto__sha256", |s: &str| crypto::sha256_hex(s.as_bytes()));
    engine.register_fn("crypto__sha512", |s: &str| crypto::sha512_hex(s.as_bytes()));
    engine.register_fn("crypto__sha1", |s: &str| crypto::sha1_hex(s.as_bytes()));
    engine.register_fn("crypto__md5", |s: &str| crypto::md5_hex(s.as_bytes()));
    engine.register_fn("crypto__hmac_sha256", |key: &str, data: &str| -> Result<String, Box<EvalAltResult>> {
        crypto::hmac_sha256_hex(key.as_bytes(), data.as_bytes()).map_err(|e| e.into())
    });
    engine.register_fn("crypto__hmac_sha512", |key: &str, data: &str| -> Result<String, Box<EvalAltResult>> {
        crypto::hmac_sha512_hex(key.as_bytes(), data.as_bytes()).map_err(|e| e.into())
    });
    engine.register_fn("crypto__constant_time_eq", |a: &str, b: &str| {
        crypto::constant_time_eq(a.as_bytes(), b.as_bytes())
    });

    // --- rng ---
    engine.register_fn("rng__bytes", |n: i64| -> Result<Dynamic, Box<EvalAltResult>> {
        let n = usize::try_from(n).map_err(|e| e.to_string())?;
        let bytes = rng::random_bytes(n).map_err(|e| e.to_string())?;
        Ok(Dynamic::from(
            bytes.into_iter().map(|b| Dynamic::from(b as i64)).collect::<Vec<_>>(),
        ))
    });
    engine.register_fn("rng__u64", |max_exclusive: i64| -> Result<i64, Box<EvalAltResult>> {
        if max_exclusive <= 0 {
            return Err("rng::u64 bound must be > 0".into());
        }
        let v = rng::random_u64_bounded(max_exclusive as u64).map_err(|e| e.to_string())?;
        Ok(v as i64)
    });
    engine.register_fn("rng__uuid", || rng::random_uuid_v4());

    // --- encoding ---
    engine.register_fn("encoding__hex_encode", |s: &str| encoding::hex_encode(s.as_bytes()));
    engine.register_fn("encoding__hex_decode", |s: &str| -> Result<String, Box<EvalAltResult>> {
        let bytes = encoding::hex_decode(s).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string().into())
    });
    engine.register_fn("encoding__base64_encode", |s: &str| encoding::base64_encode(s.as_bytes()));
    engine.register_fn("encoding__base64_decode", |s: &str| -> Result<String, Box<EvalAltResult>> {
        let bytes = encoding::base64_decode(s).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string().into())
    });

    // --- http ---
    const DEFAULT_HTTP_MAX_BODY: u64 = 16 * 1024 * 1024;
    engine.register_fn(
        "http__request",
        |method: &str, url: &str, headers: Map, body: Dynamic, timeout_ms: i64| -> Result<Map, Box<EvalAltResult>> {
            let mut hdrs = std::collections::BTreeMap::new();
            for (k, v) in headers.iter() {
                hdrs.insert(k.to_string(), v.to_string());
            }
            let body_opt = if body.is_unit() {
                None
            } else {
                let s = body.to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            };
            let body_ref = body_opt.as_deref();
            let resp = http::request(
                method,
                url,
                &hdrs,
                body_ref,
                timeout_ms.max(1) as u64,
                DEFAULT_HTTP_MAX_BODY,
            )
            .map_err(|e| e.to_string())?;
            let mut out = Map::new();
            out.insert("status".into(), Dynamic::from(resp.status));
            out.insert("body".into(), Dynamic::from(resp.body));
            let mut hmap = Map::new();
            for (k, v) in resp.headers {
                hmap.insert(k.into(), Dynamic::from(v));
            }
            out.insert("headers".into(), Dynamic::from(hmap));
            Ok(out)
        },
    );
    engine.register_fn("http__get", |url: &str| -> Result<Map, Box<EvalAltResult>> {
        let resp = http::request("GET", url, &Default::default(), None, 30_000, DEFAULT_HTTP_MAX_BODY)
            .map_err(|e| e.to_string())?;
        let mut out = Map::new();
        out.insert("status".into(), Dynamic::from(resp.status));
        out.insert("body".into(), Dynamic::from(resp.body));
        Ok(out)
    });
    engine.register_fn(
        "http__post",
        |url: &str, body: &str| -> Result<Map, Box<EvalAltResult>> {
            let resp = http::request(
                "POST",
                url,
                &Default::default(),
                Some(body),
                30_000,
                DEFAULT_HTTP_MAX_BODY,
            )
            .map_err(|e| e.to_string())?;
            let mut out = Map::new();
            out.insert("status".into(), Dynamic::from(resp.status));
            out.insert("body".into(), Dynamic::from(resp.body));
            Ok(out)
        },
    );

    let _ = handles;
}

// prelude helpers removed — module calls are rewritten via rewrite_module_calls()

/// Rewrite `log::info` → `log__info` etc. before eval.
pub fn rewrite_module_calls(src: &str) -> String {
    let mut out = src.to_string();
    let replacements = [
        ("log::info", "log__info"),
        ("log::warn", "log__warn"),
        ("log::error", "log__error"),
        ("path::cwd", "path__cwd"),
        ("path::chdir", "path__chdir"),
        ("path::resolve", "path__resolve"),
        ("path::which", "path__which"),
        ("path::is_file", "path__is_file"),
        ("path::is_dir", "path__is_dir"),
        ("path::is_absolute", "path__is_absolute"),
        ("path::join", "path__join"),
        ("path::temp_dir", "path__temp_dir"),
        ("time::utc_stamp", "time__utc_stamp"),
        ("toml::load", "toml__load"),
        ("env::get", "env__get"),
        ("env::set", "env__set"),
        ("env::remove", "env__remove"),
        ("timer::sleep", "timer__sleep"),
        ("file::exists", "file__exists"),
        ("file::read_all", "file__read_all"),
        ("file::write_all", "file__write_all"),
        ("file::append", "file__append"),
        ("file::metadata", "file__metadata"),
        ("file::copy", "file__copy"),
        ("file::move", "file__move"),
        ("file::remove", "file__remove"),
        ("dir::create", "dir__create"),
        ("dir::remove", "dir__remove"),
        ("dir::exists", "dir__exists"),
        ("dir::list", "dir__list"),
        ("dir::walk", "dir__walk"),
        ("process::run_stdin", "process__run_stdin"),
        ("process::pipeline", "process__pipeline"),
        ("process::run", "process__run"),
        ("process::run_capture", "process__run_capture"),
        ("process::run_cwd", "process__run_cwd"),
        ("process::write_stdin", "process__write_stdin"),
        ("process::close_stdin", "process__close_stdin"),
        ("pipe::new", "pipe__new"),
        ("tcp::connect", "tcp__connect"),
        ("tcp::listen", "tcp__listen"),
        ("udp::bind", "udp__bind"),
        ("cancel::request", "cancel__request"),
        ("cancel::requested", "cancel__requested"),
        ("cancel::throw_if_requested", "cancel__throw_if_requested"),
        ("execution::on_rollback", "execution__on_rollback"),
        ("execution::on_cleanup", "execution__on_cleanup"),
        ("execution::run_action", "execution__run_action"),
        ("execution::run_workflow", "execution__run_workflow"),
        ("execution::run", "execution__run"),
        ("crypto::sha256", "crypto__sha256"),
        ("crypto::sha512", "crypto__sha512"),
        ("crypto::sha1", "crypto__sha1"),
        ("crypto::md5", "crypto__md5"),
        ("crypto::hmac_sha256", "crypto__hmac_sha256"),
        ("crypto::hmac_sha512", "crypto__hmac_sha512"),
        ("crypto::constant_time_eq", "crypto__constant_time_eq"),
        ("rng::bytes", "rng__bytes"),
        ("rng::u64", "rng__u64"),
        ("rng::uuid", "rng__uuid"),
        ("encoding::hex_encode", "encoding__hex_encode"),
        ("encoding::hex_decode", "encoding__hex_decode"),
        ("encoding::base64_encode", "encoding__base64_encode"),
        ("encoding::base64_decode", "encoding__base64_decode"),
        ("http::request", "http__request"),
        ("http::get", "http__get"),
        ("http::post", "http__post"),
    ];
    for (from, to) in replacements {
        out = out.replace(from, to);
    }
    out
}

fn dynamic_to_string_vec(args: Dynamic) -> Result<Vec<String>, Box<EvalAltResult>> {
    if args.is_array() {
        let arr = args.into_array().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for v in arr {
            out.push(v.to_string());
        }
        Ok(out)
    } else if args.is_unit() {
        Ok(Vec::new())
    } else {
        Ok(vec![args.to_string()])
    }
}

fn which_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
            let cmd = dir.join(format!("{name}.cmd"));
            if cmd.is_file() {
                return Some(cmd);
            }
            let bat = dir.join(format!("{name}.bat"));
            if bat.is_file() {
                return Some(bat);
            }
        }
    }
    None
}

/// Days since Unix epoch → (year, month, day) UTC. Algorithm from civil_from_days.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
