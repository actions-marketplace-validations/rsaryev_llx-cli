//! The client over a Unix socket (W1, W4): the same frames as in process, one JSON object per
//! line. A writer thread takes what the interface sends, so a caller never waits on the socket;
//! the server's lines are read by the loop the in-process transport uses.

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use protocol::{ToClient, ToServer};

use crate::Client;

/// A server that reads nothing for this long is taken for gone, so the writer never hangs.
const WRITE_WAIT: Duration = Duration::from_secs(10);

impl Client {
    /// A client over the socket at `path`: frames from the server that are not what a command
    /// came to go to `deliver`, in order, until the connection ends. A line that is not a frame
    /// is skipped (W3: what is not known is ignored).
    pub fn connect(
        path: &Path,
        deliver: impl Fn(ToClient) + Send + 'static,
    ) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_write_timeout(Some(WRITE_WAIT))?;
        let reading = stream.try_clone()?;
        #[expect(
            clippy::disallowed_methods,
            reason = "the caller never blocks on the socket: what the person sends waits here, \
                      and a server that stops reading ends the writer within WRITE_WAIT"
        )]
        let (to_server, outgoing) = channel();
        std::thread::Builder::new()
            .name("llx-client-writer".into())
            .spawn(move || write(stream, &outgoing))?;
        let frames = BufReader::new(reading)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str(&line).ok());
        Self::reading(to_server, frames, deliver)
    }
}

/// Each frame as a line, until the client is dropped or the server stops reading; then the
/// connection is shut both ways, so the server sees the end and the reader does too.
fn write(mut stream: UnixStream, outgoing: &Receiver<ToServer>) {
    for frame in outgoing {
        let Ok(mut line) = serde_json::to_vec(&frame) else {
            continue;
        };
        line.push(b'\n');
        if stream.write_all(&line).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
