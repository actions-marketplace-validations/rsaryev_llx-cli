use std::io::Read;

use super::*;

/// GET `target` on the callback, as the browser would; the status line it answered.
fn visit(callback: &Callback, target: &str) -> String {
    let port = callback.redirect.split(':').nth(2).unwrap();
    let port: u16 = port.split('/').next().unwrap().parse().unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(stream, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer.lines().next().unwrap_or("").to_string()
}

fn spec(state: Option<&str>) -> Listen<'_> {
    Listen {
        ports: &[0],
        path: "/auth/callback",
        state,
        provider: "Test",
        app: "llx",
    }
}

fn listening(state: Option<&str>) -> Callback {
    listen(spec(state)).unwrap()
}

#[test]
fn the_code_comes_back_once_the_state_matches() {
    let callback = listening(Some("s1"));
    assert!(callback.redirect.ends_with("/auth/callback"));
    assert!(visit(&callback, "/favicon.ico").contains("404"));
    assert!(visit(&callback, "/auth/callback?code=c&state=forged").contains("400"));
    assert!(visit(&callback, "/auth/callback?state=s1").contains("400"));
    assert_eq!(
        callback.wait(Duration::from_millis(50)),
        None,
        "still waiting"
    );
    assert!(visit(&callback, "/auth/callback?code=c%2B1&state=s1").contains("200"));
    assert_eq!(
        callback.wait(Duration::from_secs(5)),
        Some(Ok("c+1".into()))
    );
}

#[test]
fn a_refusal_ends_the_wait_with_its_reason() {
    let callback = listening(None);
    let target = "/auth/callback?error=access_denied&error_description=no+thanks";
    assert!(visit(&callback, target).contains("400"));
    assert_eq!(
        callback.wait(Duration::from_secs(5)),
        Some(Err("no thanks".into()))
    );
}

#[test]
fn a_newer_sign_in_cancels_this_one() {
    let callback = listening(Some("s"));
    assert!(visit(&callback, CANCEL).contains("200"));
    let outcome = callback.wait(Duration::from_secs(5));
    assert!(matches!(outcome, Some(Err(why)) if why.contains("cancelled")));
}

#[test]
fn a_connection_that_says_nothing_does_not_hold_up_the_redirect() {
    let callback = listening_with(spec(None), Duration::from_millis(100)).unwrap();
    let port = callback.redirect.split(':').nth(2).unwrap();
    let port: u16 = port.split('/').next().unwrap().parse().unwrap();
    let _idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::Builder::new()
        .name("llx-test-browser".into())
        .spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                stream,
                "GET /auth/callback?code=c HTTP/1.1\r\nHost: x\r\n\r\n"
            )
            .unwrap();
            let _ = stream.read_to_string(&mut String::new());
        })
        .unwrap();
    assert_eq!(callback.wait(Duration::from_secs(5)), Some(Ok("c".into())));
}
