//! Purpose:
//! The push half of transaction monitoring. A program built with
//! `--apm-collector host:port` sends its transaction counters to a collector
//! (`elephc monitor --apm --listen host:port`) instead of waiting to be read:
//! an application on a phone, or any program nothing can connect to, is
//! monitored the same way as a server.
//!
//! Called from:
//! - `elephc_probe_apm_init` (`spawn`), in the program.
//! - The collector's `--listen` loop (`collector_handshake`, `Session`), in
//!   the compiler binary, which links this crate's rlib: both sides run this
//!   code, so they cannot drift.
//!
//! Key details:
//! - Mutual proof of the build key before a byte of counters leaves the
//!   program: the program sends to a holder of the key only, and the collector
//!   stores what a holder of the key sent only. The tags carry their own
//!   domain strings, so neither side can be used to answer the endpoint's
//!   handshake or the other way round.
//!
//!   program   → collector : MAGIC, VERSION, nonce_p
//!   collector → program   : nonce_c, HMAC(key, "apm-push-collector" || nonce_p || nonce_c)
//!   program   → collector : HMAC(key, "apm-push-program" || nonce_c || nonce_p)
//!
//! - Then frames, program to collector: `len (u32 BE) || ciphertext || tag`,
//!   each sealed with keys of its own, derived from the session and the
//!   frame's number, so no two frames share a keystream and a frame cannot be
//!   replayed, dropped or reordered unnoticed. The first frame is the hello
//!   (`hello\t<program>\t<pid>`), every later one an `apm::encode` snapshot:
//!   the collector diffs consecutive snapshots into windows exactly as it
//!   does the ones it reads.
//! - The counters are cumulative, so a lost connection loses nothing: the
//!   program reconnects, and the next snapshot carries everything since.

use std::io::{self, Read, Write};
use std::time::Duration;

use crate::handshake::{self, hmac_sha256, tags_equal, KEY_LEN, NONCE_LEN, TAG_LEN};

/// What a push connection opens with, so a collector tells a program from a
/// stray client at the first read.
pub const MAGIC: &[u8; 8] = b"ELEPHAPM";
/// The push protocol's version, after the magic.
pub const VERSION: u8 = 2;
/// How often a program sends its counters.
pub const INTERVAL: Duration = Duration::from_secs(5);
/// The largest frame either side accepts: a snapshot of a full route table
/// is a few hundred kilobytes.
pub const MAX_FRAME: usize = 4 << 20;
/// What the first frame of a connection starts with.
pub const HELLO: &str = "hello\t";

/// The collector's proof: it holds the key.
fn collector_tag(key: &[u8], nonce_p: &[u8], nonce_c: &[u8]) -> [u8; TAG_LEN] {
    let mut message = b"apm-push-collector".to_vec();
    message.extend_from_slice(nonce_p);
    message.extend_from_slice(nonce_c);
    hmac_sha256(key, &message)
}

/// The program's proof: it holds the key.
fn program_tag(key: &[u8], nonce_c: &[u8], nonce_p: &[u8]) -> [u8; TAG_LEN] {
    let mut message = b"apm-push-program".to_vec();
    message.extend_from_slice(nonce_c);
    message.extend_from_slice(nonce_p);
    hmac_sha256(key, &message)
}

/// One authenticated push connection: the session keys and how many frames
/// have gone through it.
pub struct Session {
    enc: [u8; TAG_LEN],
    mac: [u8; TAG_LEN],
    frames: u64,
}

impl Session {
    /// The session of the connection both nonces name.
    fn new(key: &[u8], nonce_p: &[u8], nonce_c: &[u8]) -> Self {
        let mut message = b"elephc-apm-push-v1".to_vec();
        message.extend_from_slice(nonce_p);
        message.extend_from_slice(nonce_c);
        let session = hmac_sha256(key, &message);
        Self {
            enc: hmac_sha256(&session, b"enc"),
            mac: hmac_sha256(&session, b"mac"),
            frames: 0,
        }
    }

    /// The keys of the next frame, which it alone is sealed with.
    fn next_keys(&mut self) -> ([u8; TAG_LEN], [u8; TAG_LEN]) {
        let mut label = b"frame".to_vec();
        label.extend_from_slice(&self.frames.to_be_bytes());
        self.frames += 1;
        (hmac_sha256(&self.enc, &label), hmac_sha256(&self.mac, &label))
    }

    /// Seals and writes one frame.
    pub fn write_frame(&mut self, stream: &mut impl Write, payload: &[u8]) -> io::Result<()> {
        if payload.len() > MAX_FRAME {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
        }
        let (enc, mac) = self.next_keys();
        let (ciphertext, tag) = handshake::seal(&enc, &mac, payload);
        let mut frame = Vec::with_capacity(4 + ciphertext.len() + TAG_LEN);
        frame.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
        frame.extend_from_slice(&ciphertext);
        frame.extend_from_slice(&tag);
        stream.write_all(&frame)?;
        stream.flush()
    }

    /// Reads and opens one frame; an error for a frame the session's holder
    /// did not seal, or not in this place.
    pub fn read_frame(&mut self, stream: &mut impl Read) -> io::Result<Vec<u8>> {
        let mut len = [0u8; 4];
        stream.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len) as usize;
        if len > MAX_FRAME {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }
        let mut ciphertext = vec![0u8; len];
        stream.read_exact(&mut ciphertext)?;
        let mut tag = [0u8; TAG_LEN];
        stream.read_exact(&mut tag)?;
        let (enc, mac) = self.next_keys();
        handshake::open(&enc, &mac, &ciphertext, &tag)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "frame failed authentication"))
    }
}

/// The program's side of the handshake: sends nothing of its counters to a
/// collector that cannot prove the key.
pub fn program_handshake<S: Read + Write>(
    stream: &mut S,
    key: &[u8; KEY_LEN],
    nonce_p: [u8; NONCE_LEN],
) -> io::Result<Session> {
    let mut opening = Vec::with_capacity(MAGIC.len() + 1 + NONCE_LEN);
    opening.extend_from_slice(MAGIC);
    opening.push(VERSION);
    opening.extend_from_slice(&nonce_p);
    stream.write_all(&opening)?;
    // The collector's nonce and proof in one read: one timed operation.
    let mut answer = [0u8; NONCE_LEN + TAG_LEN];
    stream.read_exact(&mut answer)?;
    let (nonce_c, proof) = answer.split_at(NONCE_LEN);
    if !tags_equal(proof, &collector_tag(key, &nonce_p, nonce_c)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the collector does not hold this build's key"));
    }
    stream.write_all(&program_tag(key, &nonce_c, &nonce_p))?;
    stream.flush()?;
    Ok(Session::new(key, &nonce_p, &nonce_c))
}

/// The collector's side of the handshake: accepts frames from a program
/// that proves the key, and from nothing else.
pub fn collector_handshake<S: Read + Write>(
    stream: &mut S,
    key: &[u8; KEY_LEN],
    nonce_c: [u8; NONCE_LEN],
) -> io::Result<Session> {
    let mut opening = [0u8; 8 + 1 + NONCE_LEN];
    stream.read_exact(&mut opening)?;
    if &opening[..8] != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not an elephc program pushing transactions"));
    }
    if opening[8] != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("push protocol version {} (this collector speaks {VERSION})", opening[8]),
        ));
    }
    let nonce_p = &opening[9..];
    let mut answer = Vec::with_capacity(NONCE_LEN + TAG_LEN);
    answer.extend_from_slice(&nonce_c);
    answer.extend_from_slice(&collector_tag(key, nonce_p, &nonce_c));
    stream.write_all(&answer)?;
    stream.flush()?;
    let mut proof = [0u8; TAG_LEN];
    stream.read_exact(&mut proof)?;
    if !tags_equal(&proof, &program_tag(key, &nonce_c, nonce_p)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the program does not hold the collector's key"));
    }
    Ok(Session::new(key, nonce_p, &nonce_c))
}

/// Who is sending, as its hello says.
#[derive(Debug, PartialEq, Eq)]
pub struct Hello {
    /// The program's name.
    pub program: String,
    /// Its process id.
    pub pid: u32,
    /// How many snapshots it delivered before this connection: 0 means the
    /// first snapshot carries everything since it started, which a collector
    /// counts; otherwise a collector meeting it for the first time may be a
    /// restarted one whose predecessor stored that, and starts from it.
    pub delivered: u64,
    /// A random number drawn once per process: what tells two programs apart
    /// when their name, address and pid agree — two containers running the
    /// same build as pid 1 behind one address (review). The same across the
    /// process's reconnections.
    pub instance: u64,
}

/// The hello a program opens its frames with: who is sending.
pub fn hello(program: &str, pid: u32, delivered: u64, instance: u64) -> String {
    let program: String = program.chars().filter(|c| !c.is_control()).take(128).collect();
    format!("{HELLO}{program}\t{pid}\t{delivered}\t{instance:016x}")
}

/// Reads a hello back.
pub fn parse_hello(frame: &[u8]) -> Option<Hello> {
    let text = std::str::from_utf8(frame).ok()?.strip_prefix(HELLO)?;
    let mut fields = text.split('\t');
    let program = fields.next()?.to_string();
    let pid = fields.next()?.parse().ok()?;
    let delivered = fields.next()?.parse().ok()?;
    let instance = u64::from_str_radix(fields.next()?, 16).ok()?;
    Some(Hello { program, pid, delivered, instance })
}

/// This process's instance number (`Hello::instance`), drawn on first use.
fn instance() -> u64 {
    static INSTANCE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *INSTANCE.get_or_init(|| {
        crate::endpoint::os_random::<8>().map_or_else(
            || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_nanos() as u64);
                // A stack address: address-space randomization places it apart even in two
                // processes of one image.
                let place = 0u8;
                fallback_instance(now, std::process::id(), std::ptr::addr_of!(place) as usize as u64)
            },
            u64::from_be_bytes,
        )
    })
}

/// The instance number of a process that cannot draw randomness: its start time, pid and a
/// stack address, mixed so that every bit of the result depends on every bit of each. Two
/// containers running one build as pid 1 and started in the same few seconds differ only in
/// the low bits of the clock; unmixed, the high half a collector labels a source with was the
/// same for both, and their counters were merged as one source's (#2499).
fn fallback_instance(now_ns: u64, pid: u32, address: u64) -> u64 {
    splitmix64(splitmix64(now_ns ^ u64::from(pid).rotate_left(32)) ^ address)
}

/// SplitMix64's finalizer: a bijection on `u64` that spreads each input bit over the output.
fn splitmix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Snapshots this process delivered to a collector, across reconnections.
static DELIVERED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The open connection to the collector, shared by the sending thread and
/// the last send at exit.
static CONNECTION: std::sync::Mutex<Option<(std::net::TcpStream, Session)>> = std::sync::Mutex::new(None);

/// A second handle on the open connection's socket, through which a stop
/// shuts it while the sending thread may hold `CONNECTION` in a blocked
/// write. Cleared with the connection, so it never holds a closed one open.
static SHUTTER: std::sync::Mutex<Option<std::net::TcpStream>> = std::sync::Mutex::new(None);

/// Points the shutter at `connection`'s socket, or at none.
fn set_shutter(connection: Option<&(std::net::TcpStream, Session)>) {
    *SHUTTER.lock().unwrap_or_else(|e| e.into_inner()) =
        connection.and_then(|(stream, _)| stream.try_clone().ok());
}

/// The collector's address, for the last send when no connection is open.
static ADDRESS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Sends one snapshot over the open connection; drops it on failure.
fn send_snapshot(connection: &mut Option<(std::net::TcpStream, Session)>) -> io::Result<()> {
    let Some((stream, session)) = connection.as_mut() else {
        return Err(io::Error::new(io::ErrorKind::NotConnected, "not connected"));
    };
    match session.write_frame(stream, crate::apm_snapshot_text().as_bytes()) {
        Ok(()) => {
            DELIVERED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        Err(error) => {
            *connection = None;
            *SHUTTER.lock().unwrap_or_else(|e| e.into_inner()) = None;
            Err(error)
        }
    }
}

/// Sends the counters one last time as the process exits, so what it did
/// since the last send — all of it, for a short run — is not lost. Over the
/// open connection, or a new one when there is none yet, within a couple of
/// seconds either way: an unreachable collector must not hold up the exit.
#[no_mangle]
pub extern "C" fn elephc_probe_apm_flush(exit_code: i32) {
    // What the exit leaves open is counted first: a transaction an `exit()`
    // left without its `finally`, and the program's run (review).
    crate::apm_runtime::close_at_exit(exit_code);
    // A forked worker inherits the sender's socket and session but not its
    // thread: sending from it put a second writer on the master's connection
    // with copied frame numbers, and the collector dropped the connection at
    // the first that disagreed (review). Its counters live in the shared
    // region, which the master's sender sends.
    if !sends_here() {
        return;
    }
    let Some(address) = ADDRESS.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    // The sending thread may hold the lock mid-send; wait for it briefly.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut connection = loop {
        match CONNECTION.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::sync::TryLockError::WouldBlock) => return,
        }
    };
    // One deadline for the lock, the connection, the handshake and the send.
    if connection.is_none() {
        *connection = open(&address, deadline).ok();
        set_shutter(connection.as_ref());
    }
    let Ok(left) = remaining(deadline) else {
        return;
    };
    if let Some((stream, _)) = connection.as_mut() {
        let _ = stream.set_write_timeout(Some(left));
    }
    let _ = send_snapshot(&mut connection);
}

/// The time left before `deadline`, as a socket timeout; an error once it
/// has passed (a zero timeout would mean "none").
pub(crate) fn remaining(deadline: std::time::Instant) -> io::Result<Duration> {
    let left = deadline.saturating_duration_since(std::time::Instant::now());
    if left.is_zero() {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "the collector did not answer in time"));
    }
    Ok(left)
}

/// Connects, proves the key and says hello, all before `deadline`: an open
/// connection. Each step gets the time left, so a collector that accepts and
/// then stops answering cannot hold a program's exit past it (review).
fn open(address: &str, deadline: std::time::Instant) -> io::Result<(std::net::TcpStream, Session)> {
    let key = crate::build_key()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "this build has no key"))?;
    let nonce = crate::endpoint::os_random::<NONCE_LEN>()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no randomness for the handshake"))?;
    let mut stream = connect(address, remaining(deadline)?)?;
    stream.set_read_timeout(Some(remaining(deadline)?))?;
    stream.set_write_timeout(Some(remaining(deadline)?))?;
    let mut session = program_handshake(&mut stream, &key, nonce)?;
    stream.set_write_timeout(Some(remaining(deadline)?))?;
    let delivered = DELIVERED.load(std::sync::atomic::Ordering::Relaxed);
    session.write_frame(&mut stream, hello(&program_name(), std::process::id(), delivered, instance()).as_bytes())?;
    Ok((stream, session))
}

/// Starts sending this program's counters to `address`, on a thread of its
/// own, until `stop`: for the life of a process, or from a library's
/// `elephc_init` to its `elephc_shutdown`. A no-op while it already runs.
pub(crate) fn spawn(address: String) {
    OWNER.store(std::process::id(), std::sync::atomic::Ordering::Release);
    *ADDRESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(address.clone());
    let mut worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    if worker.is_some() {
        return;
    }
    STOPPING.store(false, std::sync::atomic::Ordering::Release);
    *worker = std::thread::Builder::new()
        .name("elephc-apm-push".to_string())
        .spawn(move || {
            // Like the endpoint's thread: a collector going away must not
            // kill the program (SIGPIPE), and the sampler, when there is one,
            // samples the threads that run PHP.
            unsafe {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, libc::SIGPIPE);
                libc::sigaddset(&mut set, libc::SIGPROF);
                libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
            }
            let mut backoff = INTERVAL;
            while !stopping() {
                let started = std::time::Instant::now();
                if let Err(error) = send_until_closed(&address) {
                    // Once per outage, not once per attempt.
                    if backoff == INTERVAL && !stopping() {
                        eprintln!("elephc-apm: collector {address}: {error}");
                    }
                }
                // A connection that lived resets the wait; one that failed at
                // once waits longer each time, up to a minute.
                backoff = if started.elapsed() > INTERVAL * 2 {
                    INTERVAL
                } else {
                    (backoff * 2).min(Duration::from_secs(60))
                };
                pause(backoff);
            }
        })
        .ok();
}

/// Stops the sending thread and waits for it to end, so a host can unload
/// the library that holds its code (review). The open connection is shut so
/// a blocked send returns at once; an opening in progress ends within its
/// deadline (`OPEN_DEADLINE`).
pub(crate) fn stop() {
    // Not in a forked child: the socket is the master's, which a shutdown
    // would close for it too, and the thread is not this process's to join.
    if !sends_here() {
        return;
    }
    STOPPING.store(true, std::sync::atomic::Ordering::Release);
    // Through the shutter, not the connection: the sending thread holds the
    // connection's lock for the whole of a blocked write, which a stop
    // waiting for that lock could not interrupt (review).
    if let Some(stream) = SHUTTER.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    CONNECTION.lock().unwrap_or_else(|e| e.into_inner()).take();
    let worker = WORKER.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

/// Sends the last counters and stops the sending thread: a library's
/// `elephc_shutdown`, after which its host may unload it (review). A later
/// `elephc_init` starts sending again.
#[no_mangle]
pub extern "C" fn elephc_probe_apm_shutdown() {
    elephc_probe_apm_flush(0);
    stop();
}

/// The process that started the sender; a forked child is not it.
static OWNER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Whether this process started the sender, and so owns its connection.
fn sends_here() -> bool {
    OWNER.load(std::sync::atomic::Ordering::Acquire) == std::process::id()
}

/// The sending thread, while it runs.
static WORKER: std::sync::Mutex<Option<std::thread::JoinHandle<()>>> = std::sync::Mutex::new(None);

/// Asks the sending thread to end.
static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the sending thread was asked to end.
fn stopping() -> bool {
    STOPPING.load(std::sync::atomic::Ordering::Acquire)
}

/// Waits `length`, or less when the thread is asked to end.
fn pause(length: Duration) {
    let until = std::time::Instant::now() + length;
    while !stopping() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50).min(until.saturating_duration_since(std::time::Instant::now())));
    }
}

/// How long the sending thread gives an opening: also how long a stop may
/// wait for one in progress.
const OPEN_DEADLINE: Duration = Duration::from_secs(5);

/// One connection's life: connect, prove the key, say hello, then send the
/// counters every `INTERVAL` until the connection breaks or the thread is
/// asked to end.
fn send_until_closed(address: &str) -> io::Result<()> {
    let opened = open(address, std::time::Instant::now() + OPEN_DEADLINE)?;
    // Every later send gets the same ten seconds, not what the opening left.
    opened.0.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut connection = CONNECTION.lock().unwrap_or_else(|e| e.into_inner());
    if stopping() {
        return Ok(());
    }
    *connection = Some(opened);
    set_shutter(connection.as_ref());
    send_snapshot(&mut connection)?;
    drop(connection);
    loop {
        pause(INTERVAL);
        if stopping() {
            return Ok(());
        }
        send_snapshot(&mut CONNECTION.lock().unwrap_or_else(|e| e.into_inner()))?;
    }
}

/// Connects to a `host:port` collector, trying each address it resolves to.
fn connect(address: &str, timeout: Duration) -> io::Result<std::net::TcpStream> {
    let deadline = std::time::Instant::now() + timeout;
    let mut last = io::Error::new(io::ErrorKind::NotFound, "the address resolves to nothing");
    for candidate in resolve(address, timeout)? {
        match std::net::TcpStream::connect_timeout(&candidate, remaining(deadline)?) {
            Ok(stream) => return Ok(stream),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// The addresses `address` names, within `timeout`.
///
/// The lookup runs on a thread of its own: `getaddrinfo` takes no timeout,
/// and a collector named by host behind a slow or broken resolver held a
/// program's exit for the resolver's own timeout, or for good, past the
/// couple of seconds the last send promises (review). A lookup that outlasts
/// the wait is left to finish on its thread, which nothing waits for.
fn resolve(address: &str, timeout: Duration) -> io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    // A literal address needs no lookup, nor a thread.
    if let Ok(literal) = address.parse::<std::net::SocketAddr>() {
        return Ok(vec![literal]);
    }
    let (answer, answered) = std::sync::mpsc::channel();
    let owned = address.to_string();
    std::thread::Builder::new()
        .name("elephc-apm-resolve".to_string())
        .spawn(move || {
            let _ = answer.send(owned.to_socket_addrs().map(|found| found.collect::<Vec<_>>()));
        })?;
    answered.recv_timeout(timeout).unwrap_or_else(|_| {
        Err(io::Error::new(io::ErrorKind::TimedOut, "the collector's name did not resolve in time"))
    })
}

/// This program's name, for the collector to tell sources apart.
fn program_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|arg0| {
            std::path::Path::new(&arg0).file_name().map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "program".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    const KEY: [u8; KEY_LEN] = [7; KEY_LEN];

    /// Both handshakes over one socket pair, the program's on its own thread.
    fn pair(program_key: [u8; KEY_LEN], collector_key: [u8; KEY_LEN]) -> (io::Result<Session>, io::Result<Session>, UnixStream, UnixStream) {
        let (mut program, mut collector) = UnixStream::pair().unwrap();
        // A side refused waits for nothing: a refusal ends the other side's read.
        for end in [&program, &collector] {
            end.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        }
        let mut program_side = program.try_clone().unwrap();
        let thread = std::thread::spawn(move || program_handshake(&mut program_side, &program_key, [1; NONCE_LEN]));
        let collected = collector_handshake(&mut collector, &collector_key, [2; NONCE_LEN]);
        if collected.is_err() {
            // Unblock a program still waiting for a proof that is not coming.
            let _ = collector.shutdown(std::net::Shutdown::Both);
        }
        let sent = thread.join().unwrap();
        let _ = program.flush();
        (sent, collected, program, collector)
    }

    /// The two sides agree, and the frames a program sends open in order.
    #[test]
    fn frames_from_a_key_holder_open_in_order() {
        let (sent, collected, mut program, mut collector) = pair(KEY, KEY);
        let (mut sent, mut collected) = (sent.unwrap(), collected.unwrap());
        sent.write_frame(&mut program, hello("worker", 42, 3, 0xbeef).as_bytes()).unwrap();
        sent.write_frame(&mut program, b"first").unwrap();
        sent.write_frame(&mut program, b"").unwrap();
        assert_eq!(
            parse_hello(&collected.read_frame(&mut collector).unwrap()),
            Some(Hello { program: "worker".to_string(), pid: 42, delivered: 3, instance: 0xbeef })
        );
        assert_eq!(collected.read_frame(&mut collector).unwrap(), b"first");
        assert_eq!(collected.read_frame(&mut collector).unwrap(), b"");
    }

    /// A program with another key is refused by the collector, and a
    /// collector with another key receives nothing.
    #[test]
    fn another_key_is_refused_on_both_sides() {
        let (sent, collected, _, _) = pair([8; KEY_LEN], KEY);
        assert_eq!(sent.err().map(|e| e.kind()), Some(io::ErrorKind::PermissionDenied), "the collector proved the wrong key");
        assert!(collected.is_err());
    }

    /// A frame is sealed for its place: replayed, it does not open.
    #[test]
    fn a_replayed_frame_does_not_open() {
        let (sent, collected, mut program, mut collector) = pair(KEY, KEY);
        let (mut sent, mut collected) = (sent.unwrap(), collected.unwrap());
        let mut recorded = Vec::new();
        sent.write_frame(&mut recorded, b"counters").unwrap();
        program.write_all(&recorded).unwrap();
        program.write_all(&recorded).unwrap();
        assert_eq!(collected.read_frame(&mut collector).unwrap(), b"counters");
        assert_eq!(collected.read_frame(&mut collector).err().map(|e| e.kind()), Some(io::ErrorKind::InvalidData));
    }

    /// Two frames with the same content do not share a ciphertext.
    #[test]
    fn frames_do_not_share_a_keystream() {
        let mut session = Session::new(&KEY, &[1; NONCE_LEN], &[2; NONCE_LEN]);
        let (mut first, mut second) = (Vec::new(), Vec::new());
        session.write_frame(&mut first, b"the same counters").unwrap();
        session.write_frame(&mut second, b"the same counters").unwrap();
        assert_ne!(first, second);
    }

    /// The endpoint's proofs and the push proofs are different strings, so a
    /// collector cannot be made to answer a service's handshake.
    #[test]
    fn push_proofs_are_not_endpoint_proofs() {
        let (nonce_a, nonce_b) = ([1; NONCE_LEN], [2; NONCE_LEN]);
        assert_ne!(collector_tag(&KEY, &nonce_a, &nonce_b), handshake::server_tag(&KEY, &nonce_a, &nonce_b));
        for want in [b'S', b'E', b'T', b'R'] {
            assert_ne!(program_tag(&KEY, &nonce_a, &nonce_b), handshake::client_tag(&KEY, &nonce_a, &nonce_b, want));
        }
    }

    /// A collector that accepts and never answers holds an opening no longer
    /// than its deadline: the last send at exit cannot hold the exit (review).
    #[test]
    fn a_silent_collector_cannot_hold_the_opening_past_its_deadline() {
        let _guard = crate::tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        static TEST_KEY: [u8; KEY_LEN] = [9; KEY_LEN];
        let saved = crate::KEY_PTR.swap(TEST_KEY.as_ptr() as usize, std::sync::atomic::Ordering::Relaxed);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        // Accepts, then says nothing for longer than the deadline.
        let silent = std::thread::spawn(move || {
            let held = listener.accept().map(|(stream, _)| stream);
            std::thread::sleep(Duration::from_secs(3));
            drop(held);
        });
        let started = std::time::Instant::now();
        let opened = open(&address, started + Duration::from_millis(400));
        let took = started.elapsed();
        crate::KEY_PTR.store(saved, std::sync::atomic::Ordering::Relaxed);
        assert!(opened.is_err(), "nothing answered");
        assert!(took < Duration::from_secs(2), "bounded by the deadline: {took:?}");
        silent.join().unwrap();
    }

    /// A stop ends the sending thread and waits for it, even in its longest
    /// wait, so a host can unload the library afterwards; a later start runs
    /// it again (review).
    #[test]
    fn the_sending_thread_stops_and_starts_again() {
        // No build key is published under this lock, so the thread fails
        // before opening any socket and waits to retry: other tests close raw
        // descriptors, and a socket opened meanwhile could take one's number.
        let _guard = crate::tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(crate::KEY_PTR.load(std::sync::atomic::Ordering::Relaxed), 0);
        let address = "127.0.0.1:1".to_string();
        spawn(address.clone());
        assert!(WORKER.lock().unwrap().is_some());
        std::thread::sleep(Duration::from_millis(200));
        let started = std::time::Instant::now();
        stop();
        assert!(started.elapsed() < Duration::from_secs(1), "stopped within its wait: {:?}", started.elapsed());
        assert!(WORKER.lock().unwrap().is_none(), "joined");
        spawn(address);
        assert!(WORKER.lock().unwrap().is_some(), "started again");
        stop();
        assert!(WORKER.lock().unwrap().is_none());
    }

    /// A forked worker's exit sends nothing: the connection it inherited is
    /// the master's, and a second writer on it broke the frame order the
    /// collector checks (review). The process that started the sender does
    /// send at exit.
    #[test]
    fn only_the_senders_process_sends_at_exit() {
        let _guard = crate::tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        static TEST_KEY: [u8; KEY_LEN] = [9; KEY_LEN];
        let saved = crate::KEY_PTR.swap(TEST_KEY.as_ptr() as usize, std::sync::atomic::Ordering::Relaxed);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        *ADDRESS.lock().unwrap() = Some(listener.local_addr().unwrap().to_string());
        let owner = OWNER.swap(std::process::id().wrapping_add(1), std::sync::atomic::Ordering::AcqRel);

        // A forked child: the sender is another process's.
        elephc_probe_apm_flush(0);
        let child_connected = listener.accept().is_ok();

        // The sender's own process: it opens a connection to send.
        OWNER.store(std::process::id(), std::sync::atomic::Ordering::Release);
        let refusing = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if let Ok((stream, _)) = listener.accept() {
                    drop(stream);
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        });
        elephc_probe_apm_flush(0);
        let owner_connected = refusing.join().unwrap();

        OWNER.store(owner, std::sync::atomic::Ordering::Release);
        *ADDRESS.lock().unwrap() = None;
        *CONNECTION.lock().unwrap_or_else(|e| e.into_inner()) = None;
        crate::KEY_PTR.store(saved, std::sync::atomic::Ordering::Relaxed);
        assert!(!child_connected, "a forked worker sends nothing");
        assert!(owner_connected, "the sender's process sends its last counters");
    }

    /// A stop interrupts I/O blocked on the connection while the sending
    /// thread holds its lock: it waited for that lock before shutting the
    /// socket, so a send stuck on a collector that stopped reading held up a
    /// library's unload for the whole write timeout (review).
    #[test]
    fn a_stop_shuts_a_connection_whose_lock_is_held() {
        let _guard = crate::tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_server, _) = listener.accept().unwrap();
        let mut blocked = client.try_clone().unwrap();
        blocked.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        *SHUTTER.lock().unwrap() = Some(client);
        let owner = OWNER.swap(std::process::id(), std::sync::atomic::Ordering::AcqRel);
        let (held, holding) = std::sync::mpsc::channel();
        // The sending thread, in a send that only the socket's shutdown ends.
        let sender = std::thread::spawn(move || {
            let _connection = CONNECTION.lock().unwrap_or_else(|e| e.into_inner());
            held.send(()).unwrap();
            let mut byte = [0u8; 1];
            let _ = blocked.read(&mut byte);
        });
        holding.recv().unwrap();
        let started = std::time::Instant::now();
        stop();
        let took = started.elapsed();
        sender.join().unwrap();
        OWNER.store(owner, std::sync::atomic::Ordering::Release);
        STOPPING.store(false, std::sync::atomic::Ordering::Release);
        assert!(took < Duration::from_secs(2), "the stop shut the socket at once: {took:?}");
        assert!(SHUTTER.lock().unwrap().is_none());
    }

    /// A collector's address resolves within the connection's time: a literal
    /// one at once, a name through a lookup the timeout bounds (review).
    #[test]
    fn a_collector_address_resolves_within_its_time() {
        assert_eq!(resolve("127.0.0.1:9", Duration::ZERO).unwrap(), vec!["127.0.0.1:9".parse().unwrap()]);
        let named = resolve("localhost:9", Duration::from_secs(5)).unwrap();
        assert!(named.iter().all(|address| address.port() == 9 && address.ip().is_loopback()), "{named:?}");
    }

    /// Without randomness, two processes started as pid 1 a few nanoseconds apart still get
    /// instance numbers whose high halves differ, which a collector labels a source with
    /// (#2499: the unmixed fallback shared them for about four seconds).
    #[test]
    fn fallback_instances_differ_in_both_halves() {
        let start = 1_800_000_000_000_000_000u64;
        let mut highs = std::collections::HashSet::new();
        let mut lows = std::collections::HashSet::new();
        for offset in 0..64 {
            let instance = fallback_instance(start + offset, 1, 0x7ffd_0000_1000);
            highs.insert(instance >> 32);
            lows.insert(instance as u32);
        }
        assert_eq!((highs.len(), lows.len()), (64, 64));
        assert_ne!(fallback_instance(start, 1, 0x7ffd_0000_1000), fallback_instance(start, 1, 0x7ffd_0000_2000));
    }

    /// A stray client is told apart at its first bytes.
    #[test]
    fn a_stray_client_is_refused() {
        let (mut client, mut collector) = UnixStream::pair().unwrap();
        client.write_all(b"GET /health HTTP/1.1\r\nHost: collector.example\r\n\r\n").unwrap();
        let refused = collector_handshake(&mut collector, &KEY, [2; NONCE_LEN]);
        assert_eq!(refused.err().map(|e| e.kind()), Some(io::ErrorKind::InvalidData));
    }
}
