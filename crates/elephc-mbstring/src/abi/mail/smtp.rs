//! Purpose:
//! Implements the native Windows SMTP transport used when no sendmail command is configured.
//!
//! Called from:
//! - `super::send_prepared()` after MIME preparation and Core mail configuration resolution.
//!
//! Key details:
//! - SMTP replies are bounded, multiline-aware, and require the expected status class.
//! - Recipient parsing respects quoted display names; Bcc is envelope-only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::ffi::{c_char, c_int, CStr};
use std::time::Duration;

use super::PreparedMail;

#[repr(C)]
struct HostEntName { h_name: *mut c_char }

#[cfg(unix)]
unsafe extern "C" {
    fn gethostname(name: *mut c_char, len: usize) -> c_int;
    fn gethostbyname(name: *const c_char) -> *mut HostEntName;
}

#[cfg(windows)]
#[link(name = "ws2_32")]
unsafe extern "system" {
    fn gethostname(name: *mut c_char, len: c_int) -> c_int;
    fn gethostbyname(name: *const c_char) -> *mut HostEntName;
}

pub(super) struct SmtpConfig<'a> { pub host: &'a [u8], pub port: u16, pub sender: &'a [u8] }

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SmtpError { Connect, Send, Receive, Protocol, Response(Vec<u8>) }

/// Submits one prepared message through a bounded synchronous SMTP session.
pub(super) fn deliver(mail: &PreparedMail, config: SmtpConfig<'_>) -> Result<(), SmtpError> {
    let host = std::str::from_utf8(config.host).map_err(|_| SmtpError::Connect)?;
    let address = resolve_ipv4(host, if config.port == 0 { 25 } else { config.port })?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(10)).map_err(|_| SmtpError::Connect)?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|_| SmtpError::Connect)?;
    stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|_| SmtpError::Connect)?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| SmtpError::Connect)?);
    // PHP's `MailConnect()` folds every failure through this initial greeting into the
    // host/port connection diagnostic, including a negative or malformed SMTP reply.
    reply(&mut reader, 2).map_err(|_| SmtpError::Connect)?;
    let helo = [b"HELO ".as_slice(), &helo_name(), b"\r\n"].concat();
    command(&mut stream, &mut reader, &helo, 2)?;
    command(&mut stream, &mut reader, &[b"MAIL FROM:<", config.sender, b">\r\n"].concat(), 2)?;
    for recipient in &mail.recipients {
        command(&mut stream, &mut reader, &[b"RCPT TO:<".as_slice(), recipient.as_slice(), b">\r\n"].concat(), 2)?;
    }
    command(&mut stream, &mut reader, b"DATA\r\n", 3)?;
    stream.write_all(&dot_stuff(&mail.smtp_bytes)).map_err(|_| SmtpError::Send)?;
    stream.write_all(b"\r\n.\r\n").map_err(|_| SmtpError::Send)?;
    reply(&mut reader, 2)?;
    let _ = stream.write_all(b"QUIT\r\n");
    Ok(())
}

/// Resolves the first IPv4 address in resolver order, matching PHP's Win32 `GetAddr()` path.
fn resolve_ipv4(host: &str, port: u16) -> Result<SocketAddr, SmtpError> {
    (host, port)
        .to_socket_addrs()
        .map_err(|_| SmtpError::Connect)?
        .find(SocketAddr::is_ipv4)
        .ok_or(SmtpError::Connect)
}

/// Resolves the local hostname to its canonical host entry, bracketing numeric addresses.
fn helo_name() -> Vec<u8> {
    let mut name = [0 as c_char; 256];
    let bytes = unsafe {
        // SAFETY: the fixed buffer is writable for its declared length and both APIs are called
        // only after TcpStream has initialized Winsock on Windows. hostent storage is borrowed
        // until the next resolver call, so its canonical name is copied immediately.
        #[cfg(unix)]
        let status = gethostname(name.as_mut_ptr(), name.len());
        #[cfg(windows)]
        let status = gethostname(name.as_mut_ptr(), name.len() as c_int);
        if status != 0 { return b"localhost".to_vec(); }
        if !name.iter().any(|byte| *byte == 0) { return b"localhost".to_vec(); }
        let host = gethostbyname(name.as_ptr());
        let ptr = if host.is_null() || (*host).h_name.is_null() { name.as_ptr() } else { (*host).h_name };
        CStr::from_ptr(ptr).to_bytes().to_vec()
    };
    if bytes.len() >= 256 || bytes.is_empty() { return b"localhost".to_vec(); }
    if std::str::from_utf8(&bytes).ok().and_then(|value| value.parse::<std::net::IpAddr>().ok()).is_some() {
        [b"[".as_slice(), &bytes, b"]"].concat()
    } else { bytes }
}

/// Writes one command and validates the response class.
fn command(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>, bytes: &[u8], class: u8) -> Result<(), SmtpError> {
    stream.write_all(bytes).map_err(|_| SmtpError::Send)?;
    reply(reader, class)
}

/// Reads a bounded SMTP reply, including RFC multiline continuations.
fn reply(reader: &mut BufReader<TcpStream>, class: u8) -> Result<(), SmtpError> {
    let mut code = None;
    for _ in 0..32 {
        let mut line = Vec::new();
        let read = reader.take(4097).read_until(b'\n', &mut line).map_err(|_| SmtpError::Receive)?;
        if read == 0 { return Err(SmtpError::Receive); }
        if line.len() > 4096 || !line.ends_with(b"\n") || line.len() < 4 || !line[..3].iter().all(u8::is_ascii_digit) { return Err(SmtpError::Protocol); }
        if line[0] - b'0' != class { return Err(SmtpError::Response(line.trim_ascii_end().to_vec())); }
        if code.as_ref().is_some_and(|expected: &[u8; 3]| expected.as_slice() != &line[..3]) { return Err(SmtpError::Protocol); }
        code.get_or_insert([line[0], line[1], line[2]]);
        if line[3] == b' ' { return Ok(()); }
        if line[3] != b'-' { return Err(SmtpError::Protocol); }
    }
    Err(SmtpError::Protocol)
}

/// Applies SMTP transparency to every line beginning with a dot.
fn dot_stuff(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut line_start = true;
    for &byte in bytes {
        if line_start && byte == b'.' { out.push(b'.'); }
        out.push(byte);
        line_start = byte == b'\n';
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Captures a complete SMTP transaction and verifies envelope and dot transparency.
    #[test]
    fn loopback_server_captures_the_expected_transcript() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut socket, peer) = listener.accept().unwrap();
            assert!(peer.is_ipv4(), "PHP's Win32 SMTP transport must connect over IPv4");
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            socket.write_all(b"220 test\r\n").unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut transcript = Vec::new();
            loop {
                let mut line = Vec::new();
                if reader.read_until(b'\n', &mut line).unwrap() == 0 { break; }
                transcript.extend_from_slice(&line);
                if line == b"DATA\r\n" { socket.write_all(b"354 go\r\n").unwrap(); }
                else if line == b".\r\n" { socket.write_all(b"250 queued\r\n").unwrap(); }
                else if line == b"QUIT\r\n" { socket.write_all(b"221 bye\r\n").unwrap(); break; }
                else if line.starts_with(b"HELO ") || line.starts_with(b"MAIL FROM:") || line.starts_with(b"RCPT TO:") { socket.write_all(b"250 ok\r\n").unwrap(); }
            }
            transcript
        });
        let mail = PreparedMail { bytes: Vec::new(), smtp_bytes: b"Subject: S\r\n\r\n.line".to_vec(),
            recipients: vec![b"to@example.test".to_vec()], sender: None, has_date: false, warnings: Vec::new() };
        deliver(&mail, SmtpConfig { host: b"localhost", port, sender: b"from@example.test" }).unwrap();
        let transcript = server.join().unwrap();
        assert!(transcript.windows(b"MAIL FROM:<from@example.test>".len()).any(|v| v == b"MAIL FROM:<from@example.test>"));
        assert!(transcript.windows(b"..line".len()).any(|v| v == b"..line"));
    }

    /// PHP's Win32 mailer creates `PF_INET` sockets and rejects an IPv6-only SMTP address.
    #[test]
    fn ipv6_only_smtp_hosts_are_rejected() {
        assert_eq!(resolve_ipv4("::1", 25), Err(SmtpError::Connect));
    }

    /// Classifies every initial-greeting failure as a connection failure like PHP.
    #[test]
    fn initial_greeting_failures_are_connect_errors() {
        for greeting in [b"554 unavailable\r\n".as_slice(), b"bad\r\n", b""] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let bytes = greeting.to_vec();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                socket.write_all(&bytes).unwrap();
            });
            let mail = PreparedMail { bytes: Vec::new(), smtp_bytes: Vec::new(), recipients: Vec::new(), sender: None, has_date: false, warnings: Vec::new() };
            assert_eq!(deliver(&mail, SmtpConfig { host: b"127.0.0.1", port, sender: b"from@example.test" }), Err(SmtpError::Connect));
            server.join().unwrap();
        }
    }

    /// Rejects EOF and malformed multiline replies without hanging or accepting partial success.
    #[test]
    fn malformed_and_truncated_replies_fail_closed() {
        for reply in [b"250".as_slice(), b"250-begin\r\ninvalid\r\n", b"250-begin\r\n", b"250-begin\r\n251 done\r\n"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let bytes = reply.to_vec();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                socket.write_all(&bytes).unwrap();
            });
            let mail = PreparedMail { bytes: Vec::new(), smtp_bytes: Vec::new(), recipients: Vec::new(), sender: None, has_date: false, warnings: Vec::new() };
            assert!(deliver(&mail, SmtpConfig { host: b"127.0.0.1", port, sender: b"from@example.test" }).is_err());
            server.join().unwrap();
        }
    }

    /// Preserves the exact negative server response at every command stage before queuing.
    #[test]
    fn every_smtp_stage_fails_closed() {
        for failed_stage in 0..=5 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                if failed_stage == 0 { socket.write_all(b"554 greeting\r\n").unwrap(); return; }
                socket.write_all(b"220 ready\r\n").unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                for stage in 1..=4 {
                    let mut line = Vec::new(); reader.read_until(b'\n', &mut line).unwrap();
                    if stage == failed_stage { socket.write_all(b"554 rejected\r\n").unwrap(); return; }
                    socket.write_all(if stage == 4 { b"354 data\r\n" } else { b"250 ok\r\n" }).unwrap();
                    if stage == 4 {
                        loop { line.clear(); reader.read_until(b'\n', &mut line).unwrap(); if line == b".\r\n" { break; } }
                        if failed_stage == 5 { socket.write_all(b"554 body\r\n").unwrap(); return; }
                        socket.write_all(b"250 queued\r\n").unwrap();
                    }
                }
                let mut line = Vec::new(); reader.read_until(b'\n', &mut line).unwrap();
                socket.write_all(b"221 bye\r\n").unwrap();
            });
            let mail = PreparedMail { bytes: Vec::new(), smtp_bytes: b"Subject: S\r\n\r\nbody".to_vec(),
                recipients: vec![b"to@example.test".to_vec()], sender: None, has_date: false, warnings: Vec::new() };
            let expected = if failed_stage == 0 {
                SmtpError::Connect
            } else if failed_stage == 5 {
                SmtpError::Response(b"554 body".to_vec())
            } else {
                SmtpError::Response(b"554 rejected".to_vec())
            };
            assert_eq!(deliver(&mail, SmtpConfig { host: b"127.0.0.1", port, sender: b"from@example.test" }), Err(expected), "stage {failed_stage}");
            server.join().unwrap();
        }
    }

    /// Treats a QUIT rejection as success once the server accepted the queued message.
    #[test]
    fn quit_failure_does_not_revoke_queued_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap(); socket.write_all(b"220 ready\r\n").unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = Vec::new(); if reader.read_until(b'\n', &mut line).unwrap() == 0 { break; }
                if line == b"DATA\r\n" { socket.write_all(b"354 data\r\n").unwrap(); }
                else if line == b".\r\n" { socket.write_all(b"250 queued\r\n").unwrap(); }
                else if line == b"QUIT\r\n" { socket.write_all(b"554 ignored\r\n").unwrap(); break; }
                else if line.starts_with(b"HELO ") || line.starts_with(b"MAIL FROM:") || line.starts_with(b"RCPT TO:") { socket.write_all(b"250 ok\r\n").unwrap(); }
            }
        });
        let mail = PreparedMail { bytes: Vec::new(), smtp_bytes: b"Subject: S\r\n\r\nbody".to_vec(), recipients: vec![b"to@example.test".to_vec()], sender: None, has_date: false, warnings: Vec::new() };
        assert!(deliver(&mail, SmtpConfig { host: b"127.0.0.1", port, sender: b"from@example.test" }).is_ok());
        server.join().unwrap();
    }
}
