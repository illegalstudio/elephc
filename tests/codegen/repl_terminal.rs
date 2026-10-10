//! Purpose:
//! Verifies the interactive REPL through an actual pseudo-terminal.
//!
//! Called from:
//! - `codegen::repl` integration tests on supported desktop hosts.
//!
//! Key details:
//! - Bounded polling prevents a broken prompt from hanging the test indefinitely.
//! - RAII closes descriptors, reaps children, and removes the fixture on failure.

use super::{elephc_cli_command, fs, Child, Fixture, Stdio};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

#[path = "repl_terminal_screen.rs"]
mod screen;

/// Owns the session process and the terminal master used to send real editing keys.
struct Terminal {
    child: Child,
    master: File,
    pending: String,
    screen: screen::Screen,
}

impl Terminal {
    /// Creates a controlling terminal with a known size and a normal ANSI terminal profile.
    fn start(fixture: &Fixture, args: &[&str]) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
        assert_eq!(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(),
            std::ptr::null(), &size) }, 0, "openpty: {}", io::Error::last_os_error());
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let mut command = elephc_cli_command(&fixture.0);
        command.args(["repl", "--php-version=8.5"]).args(args).env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap())).stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self { child: command.spawn().expect("start terminal REPL"), master,
            pending: String::new(), screen: screen::Screen::default() }
    }

    /// Returns output through a marker, retaining later bytes for the next assertion.
    fn expect(&mut self, marker: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(150);
        loop {
            if let Some(index) = self.pending.find(marker) {
                return self.pending.drain(..index + marker.len()).collect();
            }
            assert!(Instant::now() < deadline, "timed out waiting for {marker:?}: {:?}", self.pending);
            let mut poll = libc::pollfd { fd: self.master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let ready = unsafe { libc::poll(&mut poll, 1, 100) };
            if ready < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted { continue; }
            assert!(ready >= 0, "poll: {}", io::Error::last_os_error());
            if ready == 0 { continue; }
            let mut bytes = [0_u8; 4096];
            let count = self.master.read(&mut bytes).unwrap_or_else(|error| {
                panic!("terminal closed waiting for {marker:?}: {error}; {:?}", self.pending)
            });
            assert_ne!(count, 0, "terminal closed waiting for {marker:?}: {:?}", self.pending);
            let responses = self.screen.observe(&bytes[..count]);
            self.master.write_all(&responses).unwrap();
            self.pending.push_str(&String::from_utf8_lossy(&bytes[..count]));
        }
    }

    /// Sends literal terminal keys, including editing and signal-control bytes.
    fn send(&mut self, input: &[u8]) { self.master.write_all(input).unwrap(); }

    /// Waits for normal exit without blocking forever if EOF handling regresses.
    fn exited(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "terminal session: {status}; {:?}", self.pending);
                return;
            }
            assert!(Instant::now() < deadline, "terminal session did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// The mascot appears first on cold and warm terminals, and quiet/piped sessions omit it.
#[test]
fn test_repl_terminal_starts_with_mascot() {
    let fixture = Fixture::new();
    for _ in 0..2 {
        let mut terminal = Terminal::start(&fixture, &[]);
        let opening = terminal.expect(">>> ");
        assert!(opening.starts_with("\r\n        _ooOoo_\r\n"), "{opening:?}");
        assert_eq!(opening.matches("_ooOoo_").count(), 1, "{opening:?}");
        assert!(opening.contains("Elephc REPL."), "{opening:?}");
        terminal.send(b"\x04");
        terminal.exited();
    }
    let mut quiet = Terminal::start(&fixture, &["--quiet"]);
    let opening = quiet.expect(">>> ");
    assert!(!opening.contains("_ooOoo_") && !opening.contains("Elephc REPL."), "{opening:?}");
    quiet.send(b"\x04");
    quiet.exited();
    assert_eq!(super::stdout(&fixture.run("21 * 2\n", &[])), "int(42)\n");
}

/// Echo without a newline must remain on screen after the editor redraws its next prompt.
#[test]
fn test_repl_terminal_echo_preserves_visible_output() {
    let fixture = Fixture::new();
    let mut terminal = Terminal::start(&fixture, &["--quiet"]);
    terminal.expect(">>> ");
    for (source, output) in [
        ("echo 'ciao';\r", "ciao"),
        ("echo 6 * 7\r", "42"),
        ("echo 'first', 'second';\r", "firstsecond"),
        ("echo \"with newline\\n\";\r", "with newline"),
    ] {
        terminal.send(source.as_bytes());
        terminal.expect(">>> ");
        assert!(terminal.screen.has_line(output), "output disappeared after {source:?}: {:?}", terminal.screen);
    }
    terminal.send(b"\x04");
    terminal.exited();

    let piped = fixture.run("echo 'ciao';\necho 6 * 7\n", &[]);
    assert_eq!(super::stdout(&piped), "ciao42", "pipes must retain PHP's exact output bytes");
}

impl Drop for Terminal {
    /// Reaps the owned child even if an output or history assertion fails.
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

/// Exercises continuation, cancellation, editing, history recall, persistence, and clean EOF.
#[test]
fn test_repl_terminal_controls_and_history() {
    let fixture = Fixture::new();
    let mut terminal = Terminal::start(&fixture, &["--quiet"]);
    terminal.expect(">>> ");
    terminal.send(b"function cancelled() {\r");
    terminal.expect("... ");
    terminal.send(b"\x03");
    terminal.expect(">>> ");
    terminal.send(b"isseet($a)\r");
    terminal.expect("Error: Call to undefined function isseet()");
    terminal.expect(">>> ");
    for (source, diagnostic) in [
        ("das\r", "Error: eval() runtime failed"),
        ("break;\r", "Error: eval() fragment uses an unsupported construct"),
        ("eval('$a = ;');\r", "Error: eval() fragment is invalid"),
    ] {
        terminal.send(source.as_bytes());
        terminal.expect(diagnostic);
        terminal.expect(">>> ");
    }
    terminal.send(b"function twice($n) {\r");
    terminal.expect("... ");
    terminal.send(b"return $n * 2;\r");
    terminal.expect("... ");
    terminal.send(b"}\r");
    terminal.expect(">>> ");
    terminal.send(b"twice(21)\r");
    terminal.expect("int(42)");
    terminal.expect(">>> ");
    terminal.send(b"\x1b[A\r");
    terminal.expect("int(42)");
    terminal.expect(">>> ");
    terminal.send(b"20 + 23\x7f2\r");
    terminal.expect("int(42)");
    terminal.expect(">>> ");
    terminal.send(b" 12345\r");
    terminal.expect("int(12345)");
    terminal.expect(">>> ");
    terminal.send(b"\x04");
    terminal.exited();

    let path = fixture.0.join("cache-root/elephc/repl/history");
    let history = fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&history);
    assert!(text.contains("twice(21)"), "{text}");
    assert!(!text.contains("cancelled") && !text.contains("12345"), "{text}");

    let mut disabled = Terminal::start(&fixture, &["--quiet", "--no-history"]);
    disabled.expect(">>> ");
    disabled.send(b"67890\r");
    disabled.expect("int(67890)");
    disabled.expect(">>> ");
    disabled.send(b"\x04");
    disabled.exited();
    assert_eq!(fs::read(path).unwrap(), history);
}
