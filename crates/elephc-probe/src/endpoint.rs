//! Purpose:
//! The probe's remote endpoint: a Unix-socket listener that authenticates a
//! `--probe-host` client with the build-key HMAC handshake, then serves the
//! folded profile. Both sides drive `wire::*`, so the client (in the compiler)
//! and this server cannot disagree on the protocol.
//!
//! Called from:
//! - `elephc_probe_init` spawns `serve` on a background thread when
//!   `ELEPHC_PROBE_ADDR` names a socket path.
//!
//! Key details:
//! - No secret crosses the socket; only nonces and HMAC tags (see `handshake`).
//! - The server reads randomness from `/dev/urandom`; a client that cannot
//!   prove the key is disconnected before any profile bytes are sent.

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::handshake::{self, KEY_LEN, NONCE_LEN, TAG_LEN};

/// Which answer the client is asking for, sent as one byte after its proof.
///
/// The two live in the same process behind the same key, so they are one
/// protocol rather than two endpoints: an operator learns one address, and the
/// server defends one handshake.
pub const WANT_SAMPLED: u8 = b'S';
/// The exact per-function slice, rendered by the instrumentation when the next
/// one completes.
pub const WANT_EXACT: u8 = b'E';

/// How long the server waits for an exact slice to be rendered.
///
/// An exact slice exists only once something runs, so on an idle service there
/// is nothing to hand over. Bounded and answered rather than blocking: a caller
/// that waits forever cannot tell a quiet process from a broken one.
///
/// Public because the client's read timeout has to outlast it. When the two were
/// set independently the client gave up at 10s, so the documented 30-second
/// no-traffic answer could not be received at all and a slice from a request
/// completing after the tenth second was lost with it.
pub const EXACT_WAIT: Duration = Duration::from_secs(30);

extern "C" {
    /// Asks the instrumentation to hand back the next slice. Always linked
    /// beside this crate: `--with-monitoring` adds both bridges together and the
    /// `--with-<name>` surface refuses to name either alone.
    fn elephc_instr_capture_arm();
    /// Copies the rendered slice into `out` (at most `cap` bytes) and returns its
    /// true length, which may exceed `cap`; 0 when no slice is waiting.
    fn elephc_instr_capture_take(out: *mut u8, cap: usize) -> usize;
    /// Disarms a rendezvous this thread armed, so a client that gave up waiting
    /// does not leave the next request handing its profile to nobody.
    fn elephc_instr_capture_cancel();
    /// The pid of a process holding the rendezvous that the instrumentation
    /// declined to revoke because it could not tell whether that process was
    /// still writing, or 0. Only ever consulted after a capture times out.
    fn elephc_instr_capture_blocked_by() -> i32;
}

/// Arms the exact profiler and waits for the slice it renders next.
///
/// Polls rather than blocks on a condition variable: the producer is a PHP
/// request finishing on another thread, in code that must not learn about this
/// one. A slice arrives in milliseconds when traffic exists and never when it
/// does not, and the poll costs one atomic read per interval.
fn exact_slice() -> Option<String> {
    unsafe { elephc_instr_capture_arm() };
    let deadline = std::time::Instant::now() + EXACT_WAIT;
    while std::time::Instant::now() < deadline {
        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        if needed > 0 {
            // The poll above reports the length and keeps the slice, so ask
            // again with room. Only the second call consumes it.
            let mut buffer = vec![0u8; needed];
            let written = unsafe { elephc_instr_capture_take(buffer.as_mut_ptr(), needed) };
            if written == 0 {
                break;
            }
            if written <= needed {
                buffer.truncate(written);
                return String::from_utf8(buffer).ok();
            }
            // The slice grew between the peek and the take, so the take reported
            // the NEW length and consumed nothing — `buffer` still holds the
            // zeros it was allocated with. `truncate(written.min(needed))` then
            // handed those zeros back as the profile, and a run of NUL bytes is
            // valid UTF-8, so nothing downstream objected: the operator received
            // an empty-looking capture instead of the request's slice, with the
            // rendezvous still armed. Fall through to the sleep and ask again;
            // the next peek reports the length the slice actually has now.
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    unsafe { elephc_instr_capture_cancel() };
    None
}

/// One exact capture at a time, across every connection this endpoint serves.
///
/// The rendezvous is a single slot in shared memory and arming it clears what is
/// in it, so two clients asking at once take each other's answers away. The
/// comment on the arm claimed the endpoint serialized its callers; nothing did,
/// and authenticated connections run concurrently by design.
static EXACT_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The exact answer, or a line saying why there is not one.
///
/// An empty body would be indistinguishable from a service that served no
/// requests, which is the one thing an operator must not have to guess about.
fn exact_answer() -> String {
    // A poisoned lock is not a capture in progress. `try_lock` returns `Err` for
    // both, so one panic inside a capture made every later `--exact` answer
    // "another capture is in progress" for the life of the process — a sentence
    // that is false, and that an operator cannot act on because there is no
    // other operator to wait for. The state this guards is a slot in shared
    // memory that the rendezvous validates on its own, so recovering the guard
    // is safe; only the mutex's opinion of a previous panic is discarded.
    let turn = match EXACT_TURN.try_lock() {
        Ok(turn) => turn,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            return "elephc-instr: note: another exact capture is in progress; \
                    only one runs at a time\n"
                .to_string();
        }
    };
    let _turn = turn;
    match exact_slice() {
        Some(profile) if !profile.is_empty() => profile,
        // "No request completed" is one explanation for an empty wait, and it is
        // the wrong one when requests ARE completing and being refused the
        // rendezvous. A slot held by a process the instrumentation could not
        // vouch for is never taken back — the alternative is two writers in one
        // payload — so it can outlast this wait, and saying which process holds
        // it is the difference between an operator restarting that worker and an
        // operator concluding the profiler does not work.
        _ => match unsafe { elephc_instr_capture_blocked_by() } {
            0 => format!(
                "elephc-instr: note: no request completed within {}s, or its profile \
                 could not be handed over\n",
                EXACT_WAIT.as_secs()
            ),
            pid => format!(
                "elephc-instr: note: the capture slot is held by pid {pid}, which this \
                 platform cannot confirm is still writing; it is left alone rather than \
                 handed to a second writer. Restart that worker to clear it.\n"
            ),
        },
    }
}

/// Per-connection I/O timeout, so a stalled handshake ends rather than lasting
/// forever. It BOUNDS a stall; `dispatch` is what keeps the stall off the accept
/// thread. The timeout alone did not: a peer that connected and never sent its
/// nonce held the only accept thread for all ten seconds, and the next operator
/// to ask for a profile waited exactly that long (measured: 0.02s with nobody
/// parked, 10.01s with one silent peer, and outright failure with five).
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// How many connections may be mid-handshake at once.
///
/// One thread per connection is what stops a silent peer from blocking every
/// other one, but spawning without a bound just moves the denial of service from
/// the accept thread to the thread table.
///
/// Eight was the first figure — far above what profiling needs concurrently —
/// and it was too small for the wrong reason: the bound is not about how many
/// profilers there are, it is about how long a connection may sit in the table
/// before it ages out. At a few thousand connections a second, eight slots turn
/// over in under two milliseconds, which is less than it takes to schedule the
/// handler thread that would have read the first byte. So a legitimate client
/// was evicted before it could speak, whatever the eviction preferred: measured
/// under a flood of silent peers, 36 of 60 profiles got through.
///
/// Raising it to 128 was tried and REFUTED by the same measurement: 29 of 60,
/// worse than the 36 it was meant to improve. More slots means more handler
/// threads blocked on a read, and under a flood the scheduler is the scarce
/// resource, not the table. So eight stands — chosen now for the reason it
/// survives rather than the reason it was picked. The threads keep the small
/// stack the experiment gave them, since a handshake needs almost none.
const MAX_IN_FLIGHT: usize = 8;

/// How long a connection may take to prove the build key.
///
/// The handshake is three short messages and needs milliseconds; `IO_TIMEOUT` is
/// sized for SERVING a profile, which can be megabytes. Charging an
/// unauthenticated peer the serving budget is what let it sit on a slot for ten
/// seconds at no cost to itself.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// A connection the endpoint can act on after handing it to a handler thread.
///
/// Two things have to be possible from OUTSIDE the handler: ending a handshake
/// that is going nowhere (the blocked read returns once the socket is shut
/// down), and lifting the short handshake deadline once the peer has proven the
/// key and the connection has become a legitimate transfer.
trait Connection: Send + Sync + 'static {
    /// Ends a pending handshake, unblocking its read.
    fn interrupt(&self);
    /// Grants the full serving budget, once authority is proven.
    fn allow_full_timeout(&self);
}

impl Connection for std::net::TcpStream {
    /// Shuts the socket down in both directions, so a peer blocked in `read`
    /// returns instead of holding the accept thread for the full timeout.
    fn interrupt(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
    /// Restores the ordinary read/write deadlines on a connection that has
    /// proven itself, which may legitimately take the whole window.
    fn allow_full_timeout(&self) {
        let _ = self.set_read_timeout(Some(IO_TIMEOUT));
        let _ = self.set_write_timeout(Some(IO_TIMEOUT));
    }
}

impl Connection for std::os::unix::net::UnixStream {
    /// Shuts the socket down in both directions; same contract as the TCP one,
    /// since the accept thread must not depend on which transport it got.
    fn interrupt(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
    /// Restores the ordinary read/write deadlines once authority is proven.
    fn allow_full_timeout(&self) {
        let _ = self.set_read_timeout(Some(IO_TIMEOUT));
        let _ = self.set_write_timeout(Some(IO_TIMEOUT));
    }
}

/// One handshake in progress.
struct Pending {
    ticket: u64,
    control: std::sync::Arc<dyn Connection>,
    /// Whether the peer has sent anything at all.
    ///
    /// This is what separates a client from a squatter cheaply: connecting costs
    /// nothing, and the whole first attack was peers that connect and never
    /// speak. It is a fairness rule, NOT an authentication one — a hostile peer
    /// can send 32 bytes as easily as a real one — so it decides who gets
    /// sacrificed when the table is full, never who gets served.
    spoke: bool,
}

/// Handshakes in progress, oldest first.
static PENDING: std::sync::Mutex<Vec<Pending>> = std::sync::Mutex::new(Vec::new());
/// Issues the tickets above.
static TICKETS: AtomicU64 = AtomicU64::new(0);

/// Registers a handshake and evicts the oldest one when the bound is full.
///
/// Dropping the NEWEST connection was the first answer, and a review was right
/// that it only moved the target: eight peers that connect and stay silent hold
/// every slot, and the ninth caller — the operator with the key — is the one
/// refused (measured: served with four peers parked, denied with eight).
///
/// Evicting the oldest inverts that. A slot is not something an unauthenticated
/// peer can hold against a newcomer, because the newcomer takes it; what a flood
/// costs the endpoint is churn, not availability. An authenticated connection is
/// never evicted: `authenticated` removes it from this list the moment the peer
/// proves the key, so a legitimate transfer of a large profile cannot be cut off
/// by whoever connects next.
fn register(control: std::sync::Arc<dyn Connection>) -> u64 {
    let ticket = TICKETS.fetch_add(1, Ordering::Relaxed);
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    pending.push(Pending { ticket, control, spoke: false });
    while pending.len() > MAX_IN_FLIGHT {
        // Never the entry that just arrived. It has not spoken yet only because
        // its handler thread has not been scheduled, and preferring the least
        // advanced peer therefore SELECTED it: eight peers that each sent one
        // nonce and stalled made the newcomer the only silent entry, so the key
        // holder was evicted on arrival every time — measured at 0 of 20, an
        // attack costing eight connections and 256 bytes.
        let older = pending.len() - 1;
        // Among the rest: the one that has said nothing goes first, oldest among
        // those; only if every older peer has spoken does the oldest overall lose
        // its place. Evicting purely by age cut off legitimate clients
        // mid-handshake — 20 of 60 under a connection flood.
        let victim = pending[..older]
            .iter()
            .position(|p| !p.spoke)
            .unwrap_or(0);
        pending.remove(victim).control.interrupt();
    }
    ticket
}

/// Records that a pending peer has sent its first bytes.
fn spoke(ticket: u64) {
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = pending.iter_mut().find(|p| p.ticket == ticket) {
        entry.spoke = true;
    }
}

/// Removes a handshake from the pending list, by ticket. Idempotent: a handler
/// calls it when it authenticates and again when it ends.
fn retire(ticket: u64) -> bool {
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    match pending.iter().position(|p| p.ticket == ticket) {
        Some(at) => {
            pending.remove(at);
            true
        }
        None => false,
    }
}

/// What a handler tells the endpoint about the connection it is holding.
///
/// Two facts, and they are not the same one. `spoke` says the peer sent
/// something, which only decides who is sacrificed when the table is full.
/// `authenticated` says it proved the build key, which takes the connection out
/// of the pending list for good and grants the serving budget.
struct Gate {
    ticket: u64,
    control: std::sync::Arc<dyn Connection>,
}

impl Gate {
    /// The peer has sent its first bytes.
    fn spoke(&self) {
        spoke(self.ticket);
    }

    /// Proves authority and leaves the evictable registry as ONE step.
    ///
    /// Reporting the race was not closing it: between a tag comparing equal and
    /// the ticket being retired, a connection arriving could still evict the
    /// socket that had just proven itself, and the key holder got nothing. The
    /// window was nanoseconds, which is an argument about likelihood rather than
    /// about correctness.
    ///
    /// So the comparison runs while the registry is held. `verify` is an HMAC
    /// over 64 bytes and a constant-time compare — microseconds, on a path taken
    /// once per connection — and eviction takes the same lock, so no arrival can
    /// interleave between proving and retiring.
    fn authenticate(&self, verify: impl FnOnce() -> bool) -> bool {
        let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        let Some(at) = pending.iter().position(|p| p.ticket == self.ticket) else {
            // Already evicted before the proof completed: the socket is shut
            // down, so there is nothing to serve into.
            return false;
        };
        if !verify() {
            return false;
        }
        pending.remove(at);
        drop(pending);
        self.control.allow_full_timeout();
        true
    }
}

/// Hands one accepted connection to its own thread.
///
/// `handle` is the shutdown/timeout control for the same connection the handler
/// owns, so the endpoint can end a handshake that is going nowhere and relax the
/// deadline once one succeeds.
///
/// One thread per connection with no cap on the THREADS looks unbounded, and a
/// review read it that way. What bounds it is the eviction above: a pending
/// handshake past `MAX_IN_FLIGHT` is interrupted, an interrupted socket makes
/// its blocked read return at once, and the thread exits — so the table tracks
/// the pending list rather than the arrival rate. Measured on the endpoint under
/// sustained load: 148,818 connections over 30 seconds (~5,000/s) peaked at 23
/// live threads and settled back to 2, with the first third of the run reaching
/// a higher mark than the last, so it does not drift. Whoever changes the
/// eviction should re-measure that; it is the only thing keeping this bounded.
///
/// Authenticated sessions are deliberately outside it: they leave the pending
/// list so a large transfer cannot be cut off, which means their threads are
/// limited only by how many peers hold the build key.
fn dispatch<S, H>(stream: S, handle: std::sync::Arc<dyn Connection>, handler: H)
where
    S: Send + 'static,
    H: FnOnce(S, &Gate) + Send + 'static,
{
    let ticket = register(handle.clone());
    // The endpoint thread blocks SIGPIPE before it accepts anything, and a new
    // thread inherits the creating thread's signal mask, so a handler keeps that
    // protection: a client that disconnects mid-write still gets EPIPE rather
    // than killing the profiled process.
    let spawned = std::thread::Builder::new()
        .name("elephc-probe-conn".to_string())
        // A handshake reads 32 bytes, hashes them, and writes 64; the default
        // multi-megabyte stack is reserved address space this never touches.
        .stack_size(256 * 1024)
        .spawn(move || {
            let gate = Gate { ticket, control: handle };
            handler(stream, &gate);
            retire(ticket);
        });
    if spawned.is_err() {
        retire(ticket);
    }
}

/// Upper bound on a served profile, enforced by the client so a buggy or hostile
/// server cannot make it allocate gigabytes from a 4-byte length.
pub const MAX_PROFILE_BYTES: usize = 64 * 1024 * 1024;

/// Wire protocol shared by the endpoint server and the `--probe-host` client.
///
/// After a successful mutual handshake the server frames the folded profile as
/// a 4-byte big-endian length followed by that many UTF-8 bytes.
pub mod wire {
    use super::*;

    /// Reads exactly `n` bytes or returns an error on short read.
    pub fn read_exact_vec(stream: &mut impl Read, n: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Client side of the handshake over an established stream: proves authority
    /// with the key and, on success, returns the served folded profile text.
    pub fn client_handshake_and_fetch(
        stream: &mut (impl Read + Write),
        key: &[u8; KEY_LEN],
        nonce_c: &[u8; NONCE_LEN],
        want: u8,
    ) -> std::io::Result<String> {
        stream.write_all(nonce_c)?;
        stream.flush()?;
        let nonce_s = read_exact_vec(stream, NONCE_LEN)?;
        let server_tag = read_exact_vec(stream, TAG_LEN)?;
        let expected = handshake::server_tag(key, nonce_c, &nonce_s);
        if !handshake::tags_equal(&server_tag, &expected) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "probe endpoint failed to prove the build key (wrong binary or key)",
            ));
        }
        // The mode goes first so the server has it before it verifies, and the
        // proof covers it: a byte flipped in flight fails the tag instead of
        // selecting a different answer.
        let client_tag = handshake::client_tag(key, &nonce_s, nonce_c, want);
        stream.write_all(&[want])?;
        stream.write_all(&client_tag)?;
        stream.flush()?;
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_PROFILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "probe profile exceeds the size cap (buggy or hostile server?)",
            ));
        }
        let ciphertext = read_exact_vec(stream, len)?;
        let payload_tag = read_exact_vec(stream, TAG_LEN)?;
        // The profile is encrypted under keys derived from the build key and
        // both nonces, so proving authority is no longer enough to READ it: a
        // passive observer holds ciphertext, and a relay that replays someone
        // else's handshake holds keys for a session it cannot produce.
        let (k_enc, k_mac) = handshake::session_keys(key, nonce_c, &nonce_s, want);
        let payload = handshake::open(&k_enc, &k_mac, &ciphertext, &payload_tag).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "probe payload failed authentication (tampered, or not this build)",
            )
        })?;
        String::from_utf8(payload)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "non-UTF-8 profile"))
    }
}

/// Frees an endpoint path for `bind` by removing a stale socket — and nothing else.
///
/// Returns the reason to refuse when the path holds something this process must
/// not delete. `bind` fails on a name that already exists, but "free the name"
/// is not a licence to unlink an arbitrary path: `ELEPHC_PROBE_ADDR` is operator
/// input, and a typo naming a config file or a database would otherwise be
/// answered by deleting it. `symlink_metadata` does not follow links, so a
/// symlink is refused as itself rather than acted on through whatever it points
/// at; a socket that still accepts a connection belongs to a live server, and
/// replacing it would leave that process holding a name nobody can reach.
fn clear_stale_socket(path: &str) -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        // Nothing there. That is precisely the state `bind` wants.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot be inspected ({error})")),
    };
    let kind = meta.file_type();
    if kind.is_symlink() {
        return Err("is a symlink; refusing to remove it".to_string());
    }
    if !kind.is_socket() {
        return Err("exists and is not a socket; refusing to remove it".to_string());
    }
    // On a shared directory any user can create a name, so an endpoint socket
    // this user does not own is somebody else's.
    if meta.uid() != unsafe { libc::getuid() } {
        return Err("is a socket owned by another user; refusing to remove it".to_string());
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(
            "is a live endpoint served by another process; refusing to replace it".to_string(),
        );
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("is a stale socket that cannot be removed ({error})"))
}

/// Spawns the endpoint listener on a background thread. Silent on bind failure —
/// a diagnostic must never take down the profiled process.
pub fn spawn(path: String) {
    std::thread::Builder::new()
        .name("elephc-probe-endpoint".to_string())
        .spawn(move || serve(&path))
        .ok();
}

/// Accept loop: bind the Unix socket (replacing a stale socket, and only a
/// stale socket — see `clear_stale_socket`), restrict it to the
/// owner, then handle each connection with the handshake and, on success, the
/// folded profile.
fn serve(path: &str) {
    // A broken client that disconnects mid-write must not kill the profiled
    // process: a write to a closed socket would otherwise raise SIGPIPE, whose
    // default action terminates. Rather than change the process-wide disposition
    // (which would alter the host program's own pipe semantics), block SIGPIPE
    // on THIS endpoint thread only. SIGPIPE is generated synchronously by the
    // faulting write, so with it blocked here the write returns EPIPE instead —
    // and the host's other threads keep their original SIGPIPE behavior.
    //
    // SIGPROF is blocked here for a different reason. `ITIMER_PROF` is delivered
    // to the PROCESS, and the kernel picks any thread not blocking it — so once
    // an operator asked for a sampled profile, the sampler's handler could
    // interrupt this thread and the connection threads it spawns. Two costs, and
    // the second is the serious one: the ring fills with the profiler's own
    // plumbing (a stack walk of a handshake is not the program under study), and
    // the handler runs on a thread that may be inside `malloc`, holding the
    // pending-connection lock, or mid-`memcpy` of a profile. Blocking it here
    // keeps sampling on the threads that run PHP, which is the only place it
    // means anything. Handler threads inherit this mask.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::sigaddset(&mut set, libc::SIGPROF);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    // `host:port` listens on TCP so a service can be read from another machine;
    // anything else is a filesystem path and stays a Unix socket, which is both
    // faster and unreachable from the network. The handshake is the same either
    // way — the transport changes who can *attempt* it, never who succeeds.
    if let Some(addr) = tcp_address(path) {
        serve_tcp(&addr);
        return;
    }
    if let Err(reason) = clear_stale_socket(path) {
        eprintln!("elephc-probe: endpoint '{path}' {reason}");
        return;
    }
    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(error) => {
            // Say so. Failing silently here means the operator set
            // ELEPHC_PROBE_ADDR, watched nothing happen, and had no way to tell
            // a refused bind from a program that ignores the variable — and the
            // commonest cause is invisible: a `sockaddr_un` path holds about 104
            // bytes, and a path longer than that fails with nothing to read.
            eprintln!(
                "elephc-probe: cannot serve on {path}: {error}{}",
                if path.len() > 100 {
                    format!(
                        " (the path is {} bytes; a Unix socket address holds about 104, \
                         so keep it in /tmp or /run)",
                        path.len()
                    )
                } else {
                    String::new()
                }
            );
            return;
        }
    };
    // Restrict the socket to the owner: the handshake authenticates, but there
    // is no reason to let other local users even reach it.
    unsafe {
        if let Ok(c_path) = std::ffi::CString::new(path) {
            libc::chmod(c_path.as_ptr(), 0o600);
        }
    }
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // The handshake deadline until the key is proven; the full
                // budget only afterwards.
                let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
                let Ok(control) = stream.try_clone() else {
                    continue;
                };
                // One misbehaving client must not stop the endpoint — nor
                // delay the next one, which is why this does not run inline.
                dispatch(stream, std::sync::Arc::new(control), |stream, gate| {
                    let _ = handle(stream, gate);
                });
            }
            Err(error) => match error.kind() {
                // Transient: a signal or a client that aborted before accept.
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted => continue,
                // fd exhaustion under load: back off instead of busy-looping a core.
                _ if error.raw_os_error() == Some(libc::EMFILE)
                    || error.raw_os_error() == Some(libc::ENFILE) =>
                {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                // Anything else means the listener is unusable; stop cleanly.
                _ => return,
            },
        }
    }
}

/// Interprets `spec` as a TCP address, or `None` when it is a filesystem path.
///
/// A path is the common case and must not be mistaken for a host: `/tmp/p.sock`
/// contains no colon, and a Windows-style path never reaches here. Requiring a
/// port keeps `host:port` unambiguous.
fn tcp_address(spec: &str) -> Option<String> {
    if spec.starts_with('/') || spec.starts_with('.') {
        return None;
    }
    let (host, port) = spec.rsplit_once(':')?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    Some(spec.to_string())
}

/// Accept loop over TCP. Same handshake, same failure handling as the Unix path.
///
/// Binding a port makes the endpoint reachable from the network, where the
/// handshake is the only thing standing between a stranger and a profile. That is
/// what it was built for — no secret crosses the wire and a client who cannot
/// prove the build key is disconnected before any bytes are served — but binding
/// a wildcard address is still a deployment decision, not a default: prefer
/// 127.0.0.1 and a tunnel, or a reverse proxy.
fn serve_tcp(addr: &str) {
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(error) => {
            // Say so, for the same reason the Unix path does: an operator who
            // set ELEPHC_PROBE_ADDR and saw nothing happen had no way to tell a
            // refused bind from a binary that ignores the variable. That fix
            // landed on one of the two listeners and not the other — a port
            // already in use, or one under 1024 without privileges, failed here
            // in silence.
            eprintln!("elephc-probe: cannot serve on {addr}: {error}");
            return;
        }
    };
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
                let Ok(control) = stream.try_clone() else {
                    continue;
                };
                dispatch(stream, std::sync::Arc::new(control), |stream, gate| {
                    let _ = handle(stream, gate);
                });
            }
            Err(error) => match error.kind() {
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted => continue,
                _ if error.raw_os_error() == Some(libc::EMFILE)
                    || error.raw_os_error() == Some(libc::ENFILE) =>
                {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                _ => return,
            },
        }
    }
}

/// Runs the server side of the handshake and serves the folded profile on
/// success. Returns early (dropping the connection) on any failure.
fn handle<S: std::io::Read + std::io::Write>(
    mut stream: S,
    gate: &Gate,
) -> std::io::Result<()> {
    let Some(key) = crate::sampler::build_key() else {
        return Ok(());
    };
    let nonce_c = wire::read_exact_vec(&mut stream, NONCE_LEN)?;
    // The peer speaks the protocol, so it stops being the cheapest thing to
    // sacrifice when the table fills.
    gate.spoke();
    // No entropy, no session. Everything below derives from this nonce, so
    // continuing without a real one would seal the profile under a keystream an
    // observer could reproduce.
    let Some(nonce_s) = os_random::<NONCE_LEN>() else {
        return Ok(());
    };
    let server_tag = handshake::server_tag(&key, &nonce_c, &nonce_s);
    stream.write_all(&nonce_s)?;
    stream.write_all(&server_tag)?;
    stream.flush()?;
    // Which answer was asked for, read BEFORE the proof is checked because the
    // proof covers it. A byte that is present but unrecognised gets the sampled
    // ring, which is what every client did before this byte existed; a client
    // that sends nothing at all fails this read and is disconnected, like any
    // other unfinished request.
    let want_byte = wire::read_exact_vec(&mut stream, 1)?;
    let want = want_byte.first().copied().unwrap_or(WANT_SAMPLED);
    let client_tag = wire::read_exact_vec(&mut stream, TAG_LEN)?;
    // Proving and leaving the evictable registry happen together, so a
    // connection arriving in between cannot evict a socket that has just proven
    // itself. Authority not proven — or already evicted before the proof
    // finished — disconnects without serving anything. The mode is inside the
    // proof, so a flipped byte lands here as a failed tag rather than as a
    // different answer: nothing decides what to collect until this passes.
    if !gate.authenticate(|| {
        let expected = handshake::client_tag(&key, &nonce_s, &nonce_c, want);
        handshake::tags_equal(&client_tag, &expected)
    }) {
        return Ok(());
    }
    let profile = match want {
        WANT_EXACT => exact_answer(),
        _ => {
            // Asking for the sampled answer is what starts sampled collection.
            // A service that nobody has asked carries the ring and fills nothing.
            crate::begin_sampled();
            // Then give that collection a bounded moment to produce something.
            // Reading the ring in the same breath as arming it meant the first
            // command was always answered from an empty ring, however busy the
            // service was.
            crate::sampled_answer()
        }
    };
    let (k_enc, k_mac) = handshake::session_keys(&key, &nonce_c, &nonce_s, want);
    let (sealed, payload_tag) = handshake::seal(&k_enc, &k_mac, profile.as_bytes());
    let bytes = sealed.as_slice();
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.write_all(&payload_tag)?;
    stream.flush()?;
    Ok(())
}

/// Fills `N` bytes from the OS entropy source, or refuses.
///
/// There is deliberately no fallback. This nonce goes into `session_keys`, so
/// the keystream is a function of it: repeat the nonce and you repeat the
/// keystream, and two profiles sealed under one keystream hand their XOR to
/// anyone who captured both. The fallback this replaces seeded a xorshift from
/// the wall clock — which is what a fleet of instances started together by one
/// orchestrator has in common, in exactly the containerised environments where
/// `/dev/urandom` goes missing in the first place.
///
/// The syscall is tried first because it needs no file descriptor and answers
/// where `/dev` is not mounted, which is the case the fallback existed for.
fn os_random<const N: usize>() -> Option<[u8; N]> {
    let mut out = [0u8; N];
    #[cfg(target_os = "linux")]
    {
        // Safety: writes at most `N` bytes into a buffer of `N`.
        let got = unsafe {
            libc::syscall(libc::SYS_getrandom, out.as_mut_ptr().cast::<libc::c_void>(), N, 0)
        };
        if got == N as libc::c_long {
            return Some(out);
        }
    }
    #[cfg(target_vendor = "apple")]
    {
        // Safety: same — `getentropy` fills exactly the length it is given, and
        // refuses lengths above 256 rather than writing short.
        let ok = unsafe { libc::getentropy(out.as_mut_ptr().cast::<libc::c_void>(), N) } == 0;
        if ok {
            return Some(out);
        }
    }
    let mut file = std::fs::File::open("/dev/urandom").ok()?;
    file.read_exact(&mut out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    //! The instrumentation's half of the rendezvous, stubbed so this crate's
    //! tests link on their own. A shipped binary always has the real ones.

    /// A unique scratch path for one test, without pulling in a temp-dir crate.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("elephc-probe-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    /// A typo in ELEPHC_PROBE_ADDR must not cost the operator a file.
    fn a_regular_file_at_the_endpoint_path_is_never_removed() {
        let path = scratch("regular");
        std::fs::write(&path, b"an operator's config, named by a typo").unwrap();
        let refused = clear_stale_socket(path.to_str().unwrap());
        assert!(refused.is_err(), "a regular file must not be unlinked");
        assert!(path.exists(), "the file is still there");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"an operator's config, named by a typo",
            "and its contents are untouched",
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    /// A link is judged as a link: neither it nor what it points at is removed.
    fn a_symlink_at_the_endpoint_path_is_refused_as_itself() {
        let target = scratch("symlink-target");
        let link = scratch("symlink");
        std::fs::write(&target, b"pointed at").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let refused = clear_stale_socket(link.to_str().unwrap());
        assert!(refused.is_err(), "a symlink must not be followed or removed");
        assert!(target.exists(), "the target survives");
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "and so does the link itself",
        );
        std::fs::remove_file(&link).ok();
        std::fs::remove_file(&target).ok();
    }

    #[test]
    /// A socket that still accepts belongs to a running server, which would be
    /// left holding a name nobody can reach.
    fn a_live_endpoint_is_not_stolen_from_the_process_serving_it() {
        let path = scratch("live");
        let name = path.to_str().unwrap().to_string();
        let listener = UnixListener::bind(&name).unwrap();
        let refused = clear_stale_socket(&name);
        assert!(
            refused.is_err(),
            "a socket that still answers belongs to a running server",
        );
        assert!(
            std::fs::symlink_metadata(&path).is_ok(),
            "so the name it is serving on stays put",
        );
        drop(listener);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    /// The case the guard must NOT refuse, or binding would never succeed after
    /// a crash left its socket behind.
    fn a_stale_socket_is_what_the_guard_exists_to_remove() {
        let path = scratch("stale");
        let name = path.to_str().unwrap().to_string();
        // Bind and drop: the file stays behind, nothing accepts on it. This is
        // what a crashed service leaves, and the case bind needs cleared.
        drop(UnixListener::bind(&name).unwrap());
        assert!(std::fs::symlink_metadata(&path).is_ok(), "the socket outlived its server");
        clear_stale_socket(&name).expect("a stale socket is removable");
        assert!(
            std::fs::symlink_metadata(&path).is_err(),
            "and is gone, so bind can have the name",
        );
    }

    #[test]
    /// Nothing to remove is success, not an error.
    fn a_free_name_is_already_what_bind_wants() {
        let path = scratch("absent");
        clear_stale_socket(path.to_str().unwrap())
            .expect("nothing to remove is not a failure");
    }

    /// Stub: this crate's tests link without the instrumentation runtime.
    #[no_mangle]
    extern "C" fn elephc_instr_capture_arm() {}

    /// Stub: no slice is ever waiting, which is the dormant case.
    #[no_mangle]
    extern "C" fn elephc_instr_capture_take(_out: *mut u8, _cap: usize) -> usize {
        0
    }

    /// Stub: nothing was armed, so cancelling is a no-op.
    #[no_mangle]
    extern "C" fn elephc_instr_capture_cancel() {}

    /// Stub: nothing holds a rendezvous these tests never open.
    #[no_mangle]
    extern "C" fn elephc_instr_capture_blocked_by() -> i32 {
        0
    }

    /// A second exact request while one is running is told so, not served silence.
    ///
    /// Arming the rendezvous clears it, so the second client used to take the
    /// first one's answer away and both got an empty profile — which reads like a
    /// service that had no traffic.
    #[test]
    fn a_second_exact_capture_is_refused_rather_than_taking_the_firsts_answer() {
        let held = super::EXACT_TURN.lock().expect("take the turn");
        let answer = super::exact_answer();
        drop(held);
        assert!(
            answer.contains("another exact capture is in progress"),
            "the second client was not told why it got nothing: {answer:?}"
        );
        // And the reader skips it rather than counting it as a function.
        assert!(
            !answer.contains(" calls="),
            "the note would parse as a profile row: {answer:?}"
        );
    }

    use super::*;

    /// Serializes the tests that share the pending-handshake registry.
    ///
    /// `PENDING` is global, as it must be, so two of these running at once make
    /// each other fail — they passed alone and failed in the suite, which is the
    /// signature of shared state rather than of a broken assertion.
    static DISPATCH_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Starts a test from an empty registry, so evictions depend only on what
    /// the test itself dispatches.
    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let guard = DISPATCH_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        PENDING.lock().unwrap_or_else(|e| e.into_inner()).clear();
        guard
    }

    /// A connection that records what the endpoint did to it.
    #[derive(Default)]
    struct FakeConn {
        interrupted: std::sync::atomic::AtomicBool,
        relaxed: std::sync::atomic::AtomicBool,
    }

    impl FakeConn {
        /// Whether the accept loop shut this connection down.
        fn was_interrupted(&self) -> bool {
            self.interrupted.load(Ordering::Relaxed)
        }
        /// Whether this connection was granted the full post-proof timeout.
        fn was_relaxed(&self) -> bool {
            self.relaxed.load(Ordering::Relaxed)
        }
    }

    impl Connection for FakeConn {
        /// Records the shutdown instead of performing one.
        fn interrupt(&self) {
            self.interrupted.store(true, Ordering::Relaxed);
        }
        /// Records the deadline relaxation instead of performing one.
        fn allow_full_timeout(&self) {
            self.relaxed.store(true, Ordering::Relaxed);
        }
    }

    /// A stalled connection must not delay the next one.
    ///
    /// This is the defect a review found and a measurement confirmed: with the
    /// handshake running inline on the accept thread, one peer that connected
    /// and never sent its nonce made the next `monitor` wait the full
    /// `IO_TIMEOUT` (0.02s became 10.01s), and five of them made it fail.
    ///
    /// The real `handle` cannot express that here — a test build embeds no build
    /// key, so it returns before reading anything, which is exactly the kind of
    /// test that passes without reaching the line it claims to cover. So the
    /// property is tested where it lives: `dispatch` must return while its
    /// handler is still running.
    #[test]
    fn a_stalled_connection_does_not_hold_the_accept_thread() {
        let _guard = fresh();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let (started, running) = std::sync::mpsc::channel::<()>();

        let before = std::time::Instant::now();
        dispatch((), std::sync::Arc::new(FakeConn::default()), move |(), _| {
            started.send(()).expect("handler started");
            let _ = blocked.recv();
        });
        let handed_off = before.elapsed();

        running.recv_timeout(Duration::from_secs(5)).expect("handler ran");
        assert!(
            handed_off < Duration::from_secs(1),
            "dispatch waited {handed_off:?} for a handler that had not finished"
        );
        release.send(()).ok();
    }

    /// A connection evicted before its proof completes cannot then be served.
    ///
    /// This is the race a review re-raised after the first answer only REPORTED
    /// it: the socket is already shut down, so proceeding would seal a profile
    /// into a closed connection. Proving and retiring now happen under one lock,
    /// and the outcome for a ticket that lost its place is a refusal.
    #[test]
    fn a_connection_evicted_before_its_proof_is_not_served() {
        let _guard = fresh();
        let control = std::sync::Arc::new(FakeConn::default());
        let gate = Gate { ticket: register(control.clone()), control };

        // Whatever removed it — an eviction, a cancelled handler — the ticket is
        // gone by the time the proof lands.
        assert!(retire(gate.ticket), "the ticket should have been pending");

        let mut verified = false;
        assert!(
            !gate.authenticate(|| {
                verified = true;
                true
            }),
            "a connection that lost its place was served anyway"
        );
        assert!(
            !verified,
            "the proof was computed for a connection that no longer exists"
        );
    }

    /// A pending connection that proves the key leaves the registry, so nothing
    /// arriving afterwards can evict it mid-transfer.
    #[test]
    fn proving_the_key_removes_the_connection_from_the_evictable_set() {
        let _guard = fresh();
        let control = std::sync::Arc::new(FakeConn::default());
        let gate = Gate { ticket: register(control.clone()), control: control.clone() };

        assert!(gate.authenticate(|| true), "a pending, proven connection was refused");
        assert!(control.was_relaxed(), "the serving budget was not granted");
        assert!(
            !retire(gate.ticket),
            "the ticket was still evictable after the proof"
        );
    }

    /// A wrong proof leaves the connection where it was, to be timed out or
    /// evicted like any other — it must not be quietly promoted.
    #[test]
    fn a_failed_proof_does_not_leave_the_registry() {
        let _guard = fresh();
        let control = std::sync::Arc::new(FakeConn::default());
        let gate = Gate { ticket: register(control.clone()), control: control.clone() };

        assert!(!gate.authenticate(|| false), "a failed proof was accepted");
        assert!(!control.was_relaxed(), "a failed proof got the serving budget");
        assert!(retire(gate.ticket), "a failed proof removed the ticket anyway");
    }

    /// A flood of stalled handshakes must not lock out the operator with the key.
    ///
    /// The first fix bounded concurrency and dropped the NEWEST connection, and a
    /// follow-up review was right that this only moved the target: eight silent
    /// peers held every slot and the ninth caller — the legitimate one — was the
    /// one refused (measured: served with four parked, denied with eight). The
    /// oldest pending handshake is evicted instead, so a slot is never something
    /// an unauthenticated peer can hold against a newcomer.
    #[test]
    fn a_flood_of_stalled_handshakes_cannot_lock_out_a_newcomer() {
        let _guard = fresh();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let blocked = std::sync::Arc::new(std::sync::Mutex::new(blocked));
        let (ran, ran_rx) = std::sync::mpsc::channel::<()>();

        let mut parked = Vec::new();
        for _ in 0..MAX_IN_FLIGHT {
            let conn = std::sync::Arc::new(FakeConn::default());
            parked.push(conn.clone());
            let blocked = blocked.clone();
            let ran = ran.clone();
            dispatch((), conn, move |(), _| {
                ran.send(()).ok();
                let _ = blocked.lock().expect("held").recv();
            });
        }
        // Wait until the bound is genuinely occupied rather than assuming the
        // spawns have been scheduled.
        for _ in 0..MAX_IN_FLIGHT {
            ran_rx.recv_timeout(Duration::from_secs(5)).expect("handler ran");
        }
        assert!(
            parked.iter().all(|c| !c.was_interrupted()),
            "a pending handshake was evicted before the bound was reached"
        );

        let (newcomer, newcomer_rx) = std::sync::mpsc::channel::<()>();
        dispatch(
            (),
            std::sync::Arc::new(FakeConn::default()),
            move |(), _| {
                newcomer.send(()).ok();
            },
        );
        newcomer_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the newcomer was refused instead of taking the evicted slot");
        assert!(
            parked[0].was_interrupted(),
            "the OLDEST pending handshake should have been evicted"
        );
        assert!(
            parked[1..].iter().all(|c| !c.was_interrupted()),
            "only the oldest pending handshake should have been evicted"
        );

        for _ in 0..MAX_IN_FLIGHT {
            release.send(()).ok();
        }
    }

    /// A peer that has spoken outranks one that has not, whatever their ages.
    ///
    /// Age alone was the first rule and it sacrificed clients mid-handshake: at a
    /// few thousand connections a second a legitimate client aged to the front of
    /// the table before its handler thread could read the first byte. Measured
    /// against a flood of silent peers, age-only served 5 of 60 profiles and this
    /// rule serves 24. It is a fairness rule and not an authentication one — a
    /// hostile peer can send 32 bytes as easily as a real one — so it decides who
    /// is sacrificed when the table is full, never who is served.
    #[test]
    fn a_peer_that_has_spoken_is_not_the_one_sacrificed() {
        let _guard = fresh();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let blocked = std::sync::Arc::new(std::sync::Mutex::new(blocked));
        let (ran, ran_rx) = std::sync::mpsc::channel::<()>();

        // The OLDEST connection speaks; every later one stays silent.
        let mut parked = Vec::new();
        for index in 0..MAX_IN_FLIGHT {
            let conn = std::sync::Arc::new(FakeConn::default());
            parked.push(conn.clone());
            let blocked = blocked.clone();
            let ran = ran.clone();
            dispatch((), conn, move |(), gate| {
                if index == 0 {
                    gate.spoke();
                }
                ran.send(()).ok();
                let _ = blocked.lock().expect("held").recv();
            });
        }
        for _ in 0..MAX_IN_FLIGHT {
            ran_rx.recv_timeout(Duration::from_secs(5)).expect("handler ran");
        }

        dispatch((), std::sync::Arc::new(FakeConn::default()), |(), _| {});

        assert!(
            !parked[0].was_interrupted(),
            "the oldest connection had spoken and should have been spared"
        );
        assert!(
            parked[1].was_interrupted(),
            "the oldest SILENT connection should have been the one evicted"
        );

        for _ in 0..MAX_IN_FLIGHT {
            release.send(()).ok();
        }
    }

    /// A connection is never evicted by its own arrival.
    ///
    /// Preferring the peer that has said nothing is what stops a squatter from
    /// holding a slot — but the peer that has said nothing is ALWAYS the one
    /// that just arrived, because its handler thread has not run yet. So peers
    /// that each send a single nonce and stall turn the rule against the only
    /// client it exists to protect: measured on the endpoint, eight of them
    /// denied the key holder 20 times out of 20, for the price of 256 bytes.
    #[test]
    fn a_newcomer_is_not_the_victim_of_its_own_arrival() {
        let _guard = fresh();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let blocked = std::sync::Arc::new(std::sync::Mutex::new(blocked));
        let (ran, ran_rx) = std::sync::mpsc::channel::<()>();

        // Every older peer has spoken and then stalled — one nonce each.
        let mut parked = Vec::new();
        for _ in 0..MAX_IN_FLIGHT {
            let conn = std::sync::Arc::new(FakeConn::default());
            parked.push(conn.clone());
            let blocked = blocked.clone();
            let ran = ran.clone();
            dispatch((), conn, move |(), gate| {
                gate.spoke();
                ran.send(()).ok();
                let _ = blocked.lock().expect("held").recv();
            });
        }
        for _ in 0..MAX_IN_FLIGHT {
            ran_rx.recv_timeout(Duration::from_secs(5)).expect("handler ran");
        }

        let newcomer = std::sync::Arc::new(FakeConn::default());
        dispatch((), newcomer.clone(), |(), _| {});

        assert!(
            !newcomer.was_interrupted(),
            "the arriving connection evicted itself"
        );
        assert!(
            parked[0].was_interrupted(),
            "the oldest peer should have lost its place to the newcomer"
        );

        for _ in 0..MAX_IN_FLIGHT {
            release.send(()).ok();
        }
    }

    /// Proving the key takes a connection out of reach of the eviction.
    ///
    /// Otherwise the fix would trade one denial for another: a large profile
    /// takes time to serve, and whoever connected afterwards would cut it off.
    #[test]
    fn an_authenticated_connection_is_never_evicted() {
        let _guard = fresh();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let (ready, authenticated_rx) = std::sync::mpsc::channel::<()>();
        let serving = std::sync::Arc::new(FakeConn::default());

        dispatch((), serving.clone(), move |(), gate| {
            assert!(gate.authenticate(|| true), "the gate should have retired the ticket");
            ready.send(()).ok();
            let _ = blocked.recv();
        });
        authenticated_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the handler authenticated");
        assert!(
            serving.was_relaxed(),
            "an authenticated connection must get the full serving budget"
        );

        // Now flood well past the bound; the authenticated transfer must survive.
        for _ in 0..MAX_IN_FLIGHT * 2 {
            dispatch((), std::sync::Arc::new(FakeConn::default()), |(), _| {});
        }
        assert!(
            !serving.was_interrupted(),
            "an authenticated transfer was evicted by later connections"
        );
        release.send(()).ok();
    }

    /// Neither accept loop may call `handle` inline again.
    ///
    /// The bug was not in `handle`; it was in who runs it. No executable test can
    /// see that — a test build has no key, so the handshake returns immediately
    /// either way — so the guard reads the source, the way this repository
    /// already guards properties that only the shape of the code expresses.
    #[test]
    fn both_accept_loops_hand_the_connection_off() {
        // Everything below `#[cfg(test)]` is this module quoting the strings it
        // is checking for, which would count as matches and, for the forbidden
        // one, make the guard fail on its own text.
        let whole = include_str!("endpoint.rs");
        let source = whole
            .split_once("#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("the test module marks the end of production code");
        let loops = source
            .match_indices("listener.accept()")
            .count();
        assert_eq!(loops, 2, "expected the Unix and TCP accept loops");
        assert_eq!(
            source.matches("dispatch(stream, std::sync::Arc::new(control),").count(),
            2,
            "both accept loops must hand the connection to `dispatch`"
        );
        // Forbidding `handle(stream)` outright does not work: the correct code
        // calls it too, inside the dispatch closure. The property is that every
        // such call is REACHED THROUGH a dispatch, so each one must have that
        // closure opening just above it.
        let calls: Vec<_> = source
            .match_indices("let _ = handle(stream, gate);")
            .collect();
        assert_eq!(calls.len(), 2, "expected one handshake call per accept loop");
        for (at, _) in calls {
            let lead = &source[at.saturating_sub(160)..at];
            assert!(
                lead.contains("dispatch(stream, std::sync::Arc::new(control),"),
                "an accept loop runs the handshake inline, which lets one silent \
                 peer starve every other operator:\n{lead}"
            );
        }
    }

    /// Drives both sides of the wire protocol over an in-memory duplex to prove
    /// the handshake serves the profile to a key-holder and rejects others.
    #[test]
    fn handshake_serves_the_profile_to_a_key_holder() {
        let key = [42u8; KEY_LEN];
        let nonce_c = [1u8; NONCE_LEN];
        let nonce_s = [2u8; NONCE_LEN];
        let profile = "elephc-probe: {main};hot 10\nelephc-probe-samples: 10\n";

        // Server frames: nonce_s, server_tag, then len + SEALED profile + tag,
        // after verifying client_tag.
        let server_tag = handshake::server_tag(&key, &nonce_c, &nonce_s);
        let (k_enc, k_mac) = handshake::session_keys(&key, &nonce_c, &nonce_s, WANT_SAMPLED);
        let (sealed, payload_tag) = handshake::seal(&k_enc, &k_mac, profile.as_bytes());
        let mut server_to_client = Vec::new();
        server_to_client.extend_from_slice(&nonce_s);
        server_to_client.extend_from_slice(&server_tag);
        server_to_client.extend_from_slice(&(sealed.len() as u32).to_be_bytes());
        server_to_client.extend_from_slice(&sealed);
        server_to_client.extend_from_slice(&payload_tag);
        // The frame this hands the client must not carry the profile in the
        // clear, or the mock could drift back to plaintext and stay green.
        assert!(
            !server_to_client
                .windows(profile.len())
                .any(|w| w == profile.as_bytes()),
            "the framed payload contains the profile verbatim"
        );

        let mut duplex = MockStream {
            to_read: server_to_client,
            read_pos: 0,
            written: Vec::new(),
        };
        let got = wire::client_handshake_and_fetch(&mut duplex, &key, &nonce_c, WANT_SAMPLED).unwrap();
        assert_eq!(got, profile);
        // The client wrote nonce_c, then the mode, then the proof over both
        // nonces AND the mode. The mode leads the proof because the server has
        // to know which answer was asked for before it can check a tag that
        // covers it.
        assert_eq!(&duplex.written[..NONCE_LEN], &nonce_c);
        assert_eq!(duplex.written[NONCE_LEN], WANT_SAMPLED);
        let expected_client_tag = handshake::client_tag(&key, &nonce_s, &nonce_c, WANT_SAMPLED);
        assert_eq!(
            &duplex.written[NONCE_LEN + 1..NONCE_LEN + 1 + TAG_LEN],
            &expected_client_tag
        );
        // And it is a proof OF that mode: asking for the other answer over the
        // same key and nonces produces a different tag, which is the whole
        // reason the byte is inside the transcript.
        let exact_tag = handshake::client_tag(&key, &nonce_s, &nonce_c, WANT_EXACT);
        assert_ne!(
            expected_client_tag, exact_tag,
            "the mode must change the proof, or flipping it in flight stays undetected"
        );
    }

    #[test]
    /// Authority is checked before a single profile byte is written.
    fn a_wrong_key_is_rejected_before_any_profile() {
        let real = [42u8; KEY_LEN];
        let wrong = [7u8; KEY_LEN];
        let nonce_c = [1u8; NONCE_LEN];
        let nonce_s = [2u8; NONCE_LEN];
        // Server proved identity with the REAL key.
        let server_tag = handshake::server_tag(&real, &nonce_c, &nonce_s);
        let mut server_to_client = Vec::new();
        server_to_client.extend_from_slice(&nonce_s);
        server_to_client.extend_from_slice(&server_tag);
        let mut duplex = MockStream {
            to_read: server_to_client,
            read_pos: 0,
            written: Vec::new(),
        };
        // Client holds the WRONG key: it must reject the server tag and not read a profile.
        let err = wire::client_handshake_and_fetch(&mut duplex, &wrong, &nonce_c, WANT_SAMPLED).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    struct MockStream {
        to_read: Vec<u8>,
        read_pos: usize,
        written: Vec<u8>,
    }

    impl Read for MockStream {
        /// Serves the canned server frames, advancing a cursor; a short read at
        /// the end is what a real stream does too.
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let remaining = &self.to_read[self.read_pos..];
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            self.read_pos += n;
            Ok(n)
        }
    }

    impl Write for MockStream {
        /// Records everything the client sent, so a test can assert the wire order.
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        /// Nothing is buffered, so flushing always succeeds.
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
