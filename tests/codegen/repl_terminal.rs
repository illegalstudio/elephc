//! Purpose:
//! Verifies the interactive REPL through an actual pseudo-terminal.
//!
//! Called from:
//! - `codegen::repl` integration tests on supported desktop hosts.
//!
//! Key details:
//! - Bounded polling prevents a broken prompt from hanging the test indefinitely.
//! - Exit waits keep draining output; macOS may wait for PTY output during process exit.
//! - RAII closes descriptors, reaps children, and removes the fixture on failure.

use super::{repl_cli_command, fs, Child, Fixture, Stdio};
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
    master: Option<File>,
    pending: String,
    screen: screen::Screen,
}

/// Distinguishes received bytes, a live quiet terminal, and a closed slave endpoint.
#[derive(PartialEq, Eq)]
enum OutputRead { Data, Pending, Closed }

impl Terminal {
    /// Creates a controlling terminal with a known size and a normal ANSI terminal profile.
    fn start(fixture: &Fixture, args: &[&str]) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let mut size = libc::winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
        assert_eq!(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(),
            std::ptr::null_mut(), &mut size) }, 0, "openpty: {}", io::Error::last_os_error());
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        // Only the duplicated standard streams belong in the child. Inheriting the
        // master would keep the terminal alive after the parent closes it on failure.
        for descriptor in [master.as_raw_fd(), slave.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
            assert!(flags >= 0, "fcntl: {}", io::Error::last_os_error());
            assert_eq!(unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) },
                0, "fcntl: {}", io::Error::last_os_error());
        }
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "fcntl: {}", io::Error::last_os_error());
        assert_eq!(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0, "fcntl: {}", io::Error::last_os_error());
        let mut command = repl_cli_command(&fixture.0);
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
        Self { child: command.spawn().expect("start terminal REPL"), master: Some(master),
            pending: String::new(), screen: screen::Screen::default() }
    }

    /// Polls one output chunk and services cursor queries, including during shutdown.
    fn read_output(&mut self, timeout_ms: i32) -> io::Result<OutputRead> {
        let Some(master) = self.master.as_mut() else { return Ok(OutputRead::Closed); };
        let mut poll = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted { Ok(OutputRead::Pending) }
                else { Err(error) };
        }
        if ready == 0 { return Ok(OutputRead::Pending); }
        if poll.revents & libc::POLLNVAL != 0 {
            return Err(io::Error::from_raw_os_error(libc::EBADF));
        }
        let mut bytes = [0_u8; 4096];
        let count = match master.read(&mut bytes) {
            Ok(0) => return Ok(OutputRead::Closed),
            Ok(count) => count,
            Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => {
                return Ok(OutputRead::Pending);
            }
            // Linux reports a closed PTY slave as EIO; Apple can return ordinary EOF.
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return Ok(OutputRead::Closed),
            Err(error) => return Err(error),
        };
        let responses = self.screen.observe(&bytes[..count]);
        self.pending.push_str(&String::from_utf8_lossy(&bytes[..count]));
        master.write_all(&responses)?;
        Ok(OutputRead::Data)
    }

    /// Returns output through a marker, retaining later bytes for the next assertion.
    fn expect(&mut self, marker: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(150);
        loop {
            if let Some(index) = self.pending.find(marker) {
                return self.pending.drain(..index + marker.len()).collect();
            }
            assert!(Instant::now() < deadline, "timed out waiting for {marker:?}: {:?}", self.pending);
            let output = self.read_output(100).unwrap_or_else(|error| {
                panic!("terminal closed waiting for {marker:?}: {error}; {:?}", self.pending)
            });
            assert!(output != OutputRead::Closed, "terminal closed waiting for {marker:?}: {:?}", self.pending);
        }
    }

    /// Sends literal terminal keys, including editing and signal-control bytes.
    fn send(&mut self, input: &[u8]) { self.master.as_mut().unwrap().write_all(input).unwrap(); }

    /// Drains final output while waiting for exit, retaining the EOF recovery deadline.
    fn exited(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut status = None;
        loop {
            if status.is_none() { status = self.child.try_wait().unwrap(); }
            let output = self.read_output(100).expect("read terminal output during exit");
            if output != OutputRead::Data {
                if let Some(status) = status {
                    assert!(status.success(), "terminal session: {status}; {:?}", self.pending);
                    return;
                }
            }
            assert!(Instant::now() < deadline,
                "terminal session did not exit; {} output bytes pending", self.pending.len());
            if output == OutputRead::Closed {
                std::thread::sleep(Duration::from_millis(10));
            }
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
    /// Disconnects the terminal before reaping, so macOS cannot wait on unread PTY output.
    fn drop(&mut self) {
        drop(self.master.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Exit drains a full output queue, while dropping an active session closes the PTY first.
#[test]
fn test_repl_terminal_shutdown_with_pending_output() {
    let fixture = Fixture::new();
    let mut terminal = Terminal::start(&fixture, &["--quiet", "--no-history"]);
    terminal.expect(">>> ");
    terminal.send(b"echo str_repeat('x', 262144); exit(0);\r");
    terminal.exited();
    assert!(terminal.pending.contains(&"x".repeat(262144)), "exit output was truncated");

    let mut interrupted = Terminal::start(&fixture, &["--quiet", "--no-history"]);
    interrupted.expect(">>> ");
    interrupted.send(b"echo str_repeat('y', 262144); exit(0);\r");
    interrupted.expect(&"y".repeat(128));
    // Enough output remains to block a writer or a macOS controlling-terminal exit.
    let start = Instant::now();
    drop(interrupted);
    assert!(start.elapsed() < Duration::from_secs(10), "terminal cleanup stalled");
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
