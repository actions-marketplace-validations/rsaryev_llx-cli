//! The local server a browser sign-in comes back to: it waits on 127.0.0.1 for the redirect,
//! checks its route and `state`, answers the browser with a page and hands the code over. A
//! fixed port another sign-in still holds is asked to give way through its `/cancel` route, and
//! this one gives way the same way.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

use crate::http::query_value;
use crate::page;

const CANCEL: &str = "/cancel";
const ATTEMPTS: u32 = 10;
/// How long a connection may take to say what it asks before the next one is served.
const QUIET: Duration = Duration::from_secs(2);

/// Where and how to listen.
#[derive(Debug, Clone, Copy)]
pub struct Listen<'a> {
    /// The ports to try in order; `0` takes any free one.
    pub ports: &'a [u16],
    /// The callback's route.
    pub path: &'a str,
    /// The `state` the redirect must carry, when the provider sends one back.
    pub state: Option<&'a str>,
    /// The provider signed in to, named on the pages.
    pub provider: &'a str,
    /// The app signing in, named on the success page.
    pub app: &'a str,
}

/// A server waiting for one redirect.
pub struct Callback {
    redirect: String,
    codes: Receiver<Result<String, String>>,
    stop: Arc<AtomicBool>,
    port: u16,
}

impl Callback {
    /// The address the provider is told to redirect to.
    pub fn redirect(&self) -> &str {
        &self.redirect
    }

    /// The code the browser came back with, or why it brought none; `None` while it has not.
    pub fn wait(&self, timeout: Duration) -> Option<Result<String, String>> {
        self.codes.recv_timeout(timeout).ok()
    }
}

impl Drop for Callback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept the thread is blocked in, so it sees the flag and ends.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// Wait for one redirect, on a thread of its own, at the first of `spec.ports` it can take (one
/// another sign-in holds is asked to give way).
pub fn listen(spec: Listen) -> Result<Callback, String> {
    listening_with(spec, QUIET)
}

/// [`listen`], a connection given `quiet` to say what it asks (C2: a test's is short).
fn listening_with(spec: Listen, quiet: Duration) -> Result<Callback, String> {
    let listener = bind(spec.ports)?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    // One redirect, one send: the server ends after it.
    let (sender, codes) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let serve = Served {
        path: spec.path.to_string(),
        state: spec.state.map(str::to_string),
        provider: spec.provider.to_string(),
        app: spec.app.to_string(),
        quiet,
    };
    let stopped = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("llx-sign-in".into())
        .spawn(move || serve.run(&listener, &sender, &stopped))
        .map_err(|error| format!("cannot wait for the sign-in: {error}"))?;
    Ok(Callback {
        redirect: format!("http://127.0.0.1:{port}{}", spec.path),
        codes,
        stop,
        port,
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "a backoff: the sign-in asked to give the port up has no event to wait on"
)]
fn bind(ports: &[u16]) -> Result<TcpListener, String> {
    let mut last = String::from("no port to listen on");
    for &port in ports {
        for attempt in 0..ATTEMPTS {
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(listener) => return Ok(listener),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse && port != 0 => {
                    last = format!("port {port} is in use: close the sign-in holding it");
                    if attempt == 0 {
                        ask_to_free(port);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    Err(last)
}

/// Ask a sign-in waiting on `port` (an earlier one, or another program's) to give it up.
fn ask_to_free(port: u16) {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = write!(
            stream,
            "GET {CANCEL} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        );
        let _ = std::io::Read::read(&mut stream, &mut [0u8; 64]);
    }
}

struct Served {
    path: String,
    state: Option<String>,
    provider: String,
    app: String,
    quiet: Duration,
}

impl Served {
    fn run(
        self,
        listener: &TcpListener,
        sender: &SyncSender<Result<String, String>>,
        stop: &AtomicBool,
    ) {
        for stream in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let Ok(stream) = stream else { continue };
            // One connection at a time: one that says nothing must not hold up the redirect.
            let _ = stream.set_read_timeout(Some(self.quiet));
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let target = line.split_whitespace().nth(1).unwrap_or("");
            if let Some(outcome) = self.answer(&stream, target) {
                let _ = sender.send(outcome);
                return;
            }
        }
    }

    /// Answer one request; the outcome when it ends the wait.
    fn answer(&self, stream: &TcpStream, target: &str) -> Option<Result<String, String>> {
        let route = target.split(['?', '#']).next().unwrap_or("");
        if route == CANCEL {
            respond(stream, "200 OK", "text/plain", "Sign-in cancelled");
            return Some(Err("cancelled by a newer sign-in".into()));
        }
        if route != self.path {
            let page = page::failure("Callback route not found.", None);
            respond(stream, "404 Not Found", "text/html", &page);
            return None;
        }
        if let Some(error) = query_value(target, "error") {
            let why = query_value(target, "error_description").unwrap_or(error);
            let heading = format!("{} sign-in failed.", self.provider);
            respond(
                stream,
                "400 Bad Request",
                "text/html",
                &page::failure(&heading, Some(&why)),
            );
            return Some(Err(why));
        }
        if self.state.is_some() && query_value(target, "state") != self.state {
            // A stray or forged request: refused, and the real redirect is still awaited.
            respond(stream, "400 Bad Request", "text/plain", "State mismatch");
            return None;
        }
        let Some(code) = query_value(target, "code").filter(|code| !code.is_empty()) else {
            let page = page::failure("The redirect carried no authorization code.", None);
            respond(stream, "400 Bad Request", "text/html", &page);
            return None;
        };
        let page = page::success(&self.app, &self.provider);
        respond(stream, "200 OK", "text/html", &page);
        Some(Ok(code))
    }
}

fn respond(mut stream: &TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
}

#[cfg(test)]
#[path = "loopback_tests.rs"]
mod tests;
