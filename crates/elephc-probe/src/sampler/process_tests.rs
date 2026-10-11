//! Purpose:
//! Exercises control ownership, registered atfork hooks and shared-ring replies.
//!
//! Called from:
//! - The probe crate's unit-test harness, through isolated subprocess fixtures.
//!
//! Key details:
//! - Real init installs the hook; real fork children use only atomics and syscalls.
//! - Subprocess isolation protects the harness's fd 3, signals and probe globals.

use super::*;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::Duration;

/// Supplies the runtime word normally emitted by the compiler's monitoring prologue.
#[no_mangle]
static mut elephc_monitor_active: u64 = 0;

/// The test executable has no compiler heap allocation counter to sample.
#[no_mangle]
static mut elephc_probe_allocs_ptr: u64 = 0;

/// Runs one isolated fixture; its alarm also bounds a broken control read or fork.
fn run_fixture(mode: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_tests::probe_process_child", "--nocapture"])
        .env("ELEPHC_PROBE_PROCESS_TEST", mode)
        .env_remove("ELEPHC_PROBE_ADDR")
        .output().unwrap();
    assert!(output.status.success(), "{}: {:?}\n{}\n{}", mode, output.status,
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

/// The production handshake must establish ownership before a fork-only child starts.
#[test]
fn handshake_owned_socket_is_closed_by_the_registered_fork_hook() {
    run_fixture("claimed");
}

/// A registered child hook preserves fd 3 when init did not recognize its protocol.
#[test]
fn registered_fork_hook_preserves_an_unclaimed_descriptor() {
    run_fixture("unclaimed");
}

/// Reusing fd 3 after a real handshake cannot turn a stale claim into a close.
#[test]
fn registered_fork_hook_preserves_a_reused_descriptor() {
    run_fixture("reused");
}

/// A disconnected control server clears both its claim and socket identity.
#[test]
fn control_server_disconnect_forgets_the_socket_identity() {
    run_fixture("disconnected");
}

/// Failure to start the control server clears the authenticated socket identity.
#[test]
fn control_server_spawn_failure_forgets_the_socket_identity() {
    run_fixture("spawn-failed");
}

/// A pre-existing worker observes the control server's ask and publishes a tagged reply.
#[test]
fn control_server_activates_the_shared_window_and_reads_a_forked_worker() {
    run_fixture("shared");
}

/// Moves a socket above fd 3 so fixture setup never overwrites either owned peer.
fn high_socket(socket: UnixStream) -> UnixStream {
    let fd = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
    assert!(fd >= 10);
    unsafe { UnixStream::from_raw_fd(fd) }
}

/// Installs a real socketpair peer on fd 3 and queues the protocol's activation marker.
fn channel(marker: &[u8]) -> UnixStream {
    let (peer, server) = UnixStream::pair().unwrap();
    let mut peer = high_socket(peer);
    let server = high_socket(server);
    assert_eq!(unsafe { libc::dup2(server.as_raw_fd(), CONTROL_FD) }, CONTROL_FD);
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    peer.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    peer.write_all(marker).unwrap();
    peer
}

/// Consumes and verifies the activation evidence sent by the production handshake.
fn read_ack(peer: &mut UnixStream) {
    let mut ack = [0; CONTROL_ACK.len()];
    peer.read_exact(&mut ack).unwrap();
    assert_eq!(ack, CONTROL_ACK);
}

struct ForkChild(libc::pid_t);

impl ForkChild {
    /// Reaps a fork child and checks its syscall-only assertions encoded in exit status.
    fn finish(&mut self) {
        let mut status = 0;
        assert_eq!(unsafe { libc::waitpid(self.0, &mut status, 0) }, self.0);
        self.0 = 0;
        assert!(libc::WIFEXITED(status), "fork child status: {status}");
        assert_eq!(libc::WEXITSTATUS(status), 0, "fork child assertion failed");
    }
}

impl Drop for ForkChild {
    /// Kills and reaps a worker if its parent assertion fails before normal completion.
    fn drop(&mut self) {
        if self.0 > 0 {
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                libc::waitpid(self.0, std::ptr::null_mut(), 0);
            }
        }
    }
}

/// Initializes the real probe then checks descriptors in an actual fork child.
fn fork_ownership(mode: &str, table: &[SymtabEntry]) {
    let claimed = mode != "unclaimed";
    let mut peer = channel(if claimed { CONTROL_MAGIC } else { b"ordinary-protocol" });
    unsafe { elephc_probe_init(table.as_ptr(), table.len(), std::ptr::null()) };
    assert_eq!(CONTROL_OWNED.load(Ordering::Relaxed), claimed);
    if claimed {
        read_ack(&mut peer);
        assert_ne!(unsafe { libc::fcntl(CONTROL_FD, libc::F_GETFD) } & libc::FD_CLOEXEC, 0);
        let (dev, ino) = control_identity(CONTROL_FD).unwrap();
        assert!(same_control_identity(dev, ino));
    }
    if mode == "reused" {
        let file = std::fs::File::open("/dev/null").unwrap();
        assert_eq!(unsafe { libc::dup2(file.as_raw_fd(), CONTROL_FD) }, CONTROL_FD);
    }
    if mode == "disconnected" || mode == "spawn-failed" {
        if mode == "disconnected" {
            peer.shutdown(std::net::Shutdown::Both).unwrap();
            serve_control_channel();
        } else {
            start_control_channel(CONTROL_FD, || Err(std::io::Error::other("fixture spawn failure")));
            let file = std::fs::File::open("/dev/null").unwrap();
            let replacement = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
            assert!(replacement >= 10);
            drop(file);
            assert_eq!(unsafe { libc::dup2(replacement, CONTROL_FD) }, CONTROL_FD);
            unsafe { libc::close(replacement) };
        }
        assert!(!CONTROL_OWNED.load(Ordering::Relaxed));
        assert_eq!(CONTROL_DEV.load(Ordering::Relaxed), 0);
        assert_eq!(CONTROL_INO.load(Ordering::Relaxed), 0);
    }
    let should_close = mode == "claimed";
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        // No allocator, locks, assertions or unwinding after fork.
        let closed = unsafe { libc::fcntl(CONTROL_FD, libc::F_GETFD) } < 0;
        let forgotten = !CONTROL_OWNED.load(Ordering::Relaxed)
            && CONTROL_DEV.load(Ordering::Relaxed) == 0
            && CONTROL_INO.load(Ordering::Relaxed) == 0;
        unsafe { libc::_exit(if closed == should_close && forgotten { 0 } else { 1 }) };
    }
    ForkChild(pid).finish();
    if mode == "claimed" {
        assert!(CONTROL_OWNED.load(Ordering::Relaxed), "child must not clear the parent claim");
        peer.write_all(b"P").unwrap();
        let mut byte = 0u8;
        assert_eq!(unsafe { libc::recv(CONTROL_FD, (&mut byte as *mut u8).cast(), 1, 0) }, 1);
        assert_eq!(byte, b'P', "child must close its fd without shutting down the parent socket");
    }
    unsafe { disarm_timer() };
}

/// Reads one complete production control reply with the same framing as the monitor.
fn snapshot(peer: &mut UnixStream) -> String {
    peer.write_all(&[CONTROL_SNAPSHOT_REQUEST]).unwrap();
    let mut size = [0; 4];
    peer.read_exact(&mut size).unwrap();
    let size = u32::from_le_bytes(size) as usize;
    assert!(size < 1024 * 1024);
    let mut body = vec![0; size];
    peer.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

/// Forks before the ask, then publishes one deterministic sample from shared memory.
fn shared_reply(table: &[SymtabEntry]) {
    unsafe { libc::close(CONTROL_FD) };
    unsafe { elephc_probe_init(table.as_ptr(), table.len(), std::ptr::null()) };
    let base = REGION.load(Ordering::Relaxed);
    assert_ne!(base, 0);
    assert_eq!(unsafe { region_asked(base) }.load(Ordering::Acquire), ASK_DORMANT);
    // Pre-intern only the fixture label to keep the fork child allocation-free.
    let route = unsafe { intern_route("GET /prefork") } as u64;
    let (signal, waiting) = UnixStream::pair().unwrap();
    let mut signal = high_socket(signal);
    let waiting = high_socket(waiting);
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        unsafe {
            libc::alarm(5);
            disarm_timer();
            let mut byte = 0u8;
            if libc::read(waiting.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) != 1 {
                libc::_exit(2);
            }
            observe_shared_ask();
            disarm_timer();
            if !ASKED.load(Ordering::Relaxed)
                || region_asked(base).load(Ordering::Acquire) != ASK_ACTIVE {
                libc::_exit(3);
            }
            // Publish one known worker frame through the production ring layout.
            let ticket = region_head().unwrap().fetch_add(1, Ordering::Relaxed);
            let slot = (ticket % RING_SLOTS as u64) as usize;
            region_word(base, slot, SEQ_WORD).store(slot_seq_settled(ticket) | 1, Ordering::Relaxed);
            region_word(base, slot, 0).store(1, Ordering::Relaxed);
            region_word(base, slot, 1).store(route, Ordering::Relaxed);
            region_word(base, slot, PC_WORD0).store(0x1010, Ordering::Relaxed);
            region_word(base, slot, SEQ_WORD).store(slot_seq_settled(ticket), Ordering::Release);
            libc::_exit(0);
        }
    }
    let mut child = ForkChild(pid);
    let mut peer = channel(CONTROL_MAGIC_LIVE);
    assert!(control_fd_present());
    read_ack(&mut peer);
    let server = std::thread::spawn(serve_control_channel);
    snapshot(&mut peer);
    assert_eq!(unsafe { region_asked(base) }.load(Ordering::Acquire), ASK_ACTIVE);
    signal.write_all(b"S").unwrap();
    child.finish();
    let answer = snapshot(&mut peer);
    assert!(answer.contains("elephc-probe: GET /prefork;worker_only 1"), "{answer}");
    drop(peer);
    server.join().unwrap();
    unsafe { disarm_timer() };
}

/// Isolated entry point keeps init, fd replacement and process timers out of other tests.
#[test]
fn probe_process_child() {
    let Ok(mode) = std::env::var("ELEPHC_PROBE_PROCESS_TEST") else { return };
    unsafe { libc::alarm(10) };
    let name = b"worker_only";
    let table = [
        SymtabEntry { address: 0x1000, name_ptr: name.as_ptr() as u64, name_len: name.len() as u64 },
        SymtabEntry { address: 0x2000, name_ptr: name.as_ptr() as u64, name_len: name.len() as u64 },
    ];
    if mode == "shared" {
        shared_reply(&table);
    } else {
        fork_ownership(&mode, &table);
    }
    std::process::exit(0);
}
