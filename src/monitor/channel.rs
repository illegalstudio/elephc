//! Purpose:
//! Hands a program `monitor` launches its control channel, and finds the build
//! key that authorizes the request. Possession of the channel — an inherited
//! socket on fd 3 — is the credential for a launched program, so there is
//! nothing to copy, replay, or set from another shell.
//!
//! Called from:
//! - `local::run`, before spawning the target.
//!
//! Key details:
//! - The child end is inherited across the spawn; both ends close on drop.
//! - The child acknowledges successful runtime activation on the same channel.
//! - The key comes from the `<binary>.key` sidecar, or an explicit override.

use super::*;

/// Answers one HTTP request with the current bytes of `path`. Ignores the
/// request target: this server has exactly one resource.
pub(crate) fn serve_one_request(mut stream: std::net::TcpStream, path: &str) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    {
        // Consume the request line and headers so the client can send the body
        // and read the response cleanly.
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        loop {
            let mut header = String::new();
            let n = reader.read_line(&mut header)?;
            if n == 0 || header == "\r\n" || header == "\n" {
                break;
            }
        }
    }
    let body = std::fs::read(path).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

/// Compiles a `.php` target with the monitoring capability embedded.
///
/// One function for what used to be two, because the two mechanisms are no
/// longer two commands: whichever of them ends up reading the program, the build
/// that produces it is the same build.
///
/// Deliberately *without* `--debug-info`: the embedded sampler resolves frames
/// through the symbol table the capability carries, not through DWARF, so debug
/// info buys this path nothing and only makes the compile slower. (It also used
/// to break it outright on ELF, until the inline-thunk section restore in
/// `runtime_wrappers.rs` fixed the underlying layout bug.)
pub(crate) fn compile_php_monitored(source: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate elephc: {e}"))?;
    let status = process::Command::new(exe)
        .args(["--with-monitoring", source])
        .status()
        .map_err(|e| format!("cannot run elephc: {e}"))?;
    if !status.success() {
        return Err(format!("compiling {source} with --with-monitoring failed"));
    }
    Ok(spawnable_path(source.trim_end_matches(".php")))
}

/// Creates the socketpair that tells a spawned binary it is being monitored.
///
/// The credential is the channel itself. Only this process holds the other end,
/// so there is nothing for anyone else to copy, find in a log, or replay — unlike
/// an environment variable, which every process on the machine can read, and
/// which therefore has to be signed to be safe at all.
pub(crate) fn open_control_channel() -> Option<ControlChannel> {
    open_channel_with(CONTROL_MAGIC)
}

/// The same channel, with the marker that tells the child it will be POLLED.
///
/// Only `--live` uses it. The difference costs the child a thread it would
/// otherwise park in `recv` for the whole of a run nobody intends to interrupt.
pub(crate) fn open_polled_control_channel() -> Option<ControlChannel> {
    let channel = open_channel_with(CONTROL_MAGIC_LIVE)?;
    // A read on this end must not be able to wait forever.
    //
    // The reply comes from a thread the child starts, and the child may not have
    // started one: a spawn can fail under a thread or memory limit, and the fd
    // stays open with nobody behind it. `recv` on a live peer that never writes
    // does not return — so a live view would hang on a program still running,
    // which is the one failure mode worse than a missing profile because it
    // gives no reason.
    //
    // Generous against the answer itself, which waits out a 400 ms warm-up
    // before reporting an empty ring, and finite against everything else.
    set_receive_deadline(channel.parent, 5);
    Some(channel)
}

fn open_channel_with(magic: &[u8]) -> Option<ControlChannel> {
    unsafe {
        let mut fds = [0i32; 2];
        if libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) != 0 {
            return None;
        }
        let channel = ControlChannel {
            parent: fds[0],
            child: fds[1],
        };
        // Written BEFORE the fork, so the marker is waiting in the buffer rather
        // than racing the child's init.
        let wrote = libc::send(
            channel.parent,
            magic.as_ptr() as *const libc::c_void,
            magic.len(),
            0,
        );
        if wrote != magic.len() as isize {
            return None;
        }
        Some(channel)
    }
}

/// Whether the child acknowledged consuming the control marker and activating
/// its embedded monitoring runtime.
///
/// `MSG_PEEK`, and that is the whole contract: this is a QUESTION about the
/// stream, and it shares that stream with `request_snapshot`. It used to answer
/// by CONSUMING, under `MSG_WAITALL` — which asks the kernel to hold on until
/// all twenty bytes are there, and which `MSG_DONTWAIT` does not override on
/// macOS. Measured on a socketpair already holding one four-byte reply, the
/// shortest there is, since an empty snapshot is a length word and nothing else:
///
/// | flags | the probe | the reply afterwards |
/// |---|---|---|
/// | `DONTWAIT\|WAITALL`, no deadline | never returns | — |
/// | `DONTWAIT\|WAITALL`, 1 s deadline | returns after 1.002 s | DESTROYED |
/// | `DONTWAIT\|PEEK` | returns in 1.9 µs | intact |
///
/// So asking whether the child had activated spent a whole receive deadline and
/// swallowed the answer it was not looking for. The next length parsed came out
/// of the middle of a message, which reads as `Gone`, and `Gone` ends the live
/// view and reaps a program that was replying perfectly well. On the channel
/// that carries no deadline — the check made once the program has finished — it
/// did not come back at all.
///
/// Leaving the ACK where it is costs nothing, because `request_snapshot` already
/// has to cope with finding one in front of a reply — a child that boots slower
/// than the activation deadline puts it there — and now always takes that path
/// rather than only under load. One consumer of the stream, and it is the one
/// that knows the framing.
pub(crate) fn control_channel_activated(channel: &ControlChannel) -> bool {
    let mut ack = [0u8; CONTROL_ACK.len()];
    let read = unsafe {
        libc::recv(
            channel.parent,
            ack.as_mut_ptr() as *mut libc::c_void,
            ack.len(),
            libc::MSG_DONTWAIT | libc::MSG_PEEK,
        )
    };
    read == ack.len() as isize && ack == CONTROL_ACK
}

/// Waits, briefly, for the child's activation ACK to arrive.
///
/// `control_channel_activated` asks without blocking, which is right for a
/// caller that has already waited for the program to finish. A live view has
/// not: it starts asking while the child is still booting, and the ACK is sent
/// once, at init.
///
/// What it answers is "has the child reached its activation point", not "is the
/// stream ready to read". Both messages share one stream, and the reply is
/// length-prefixed, so an ACK in front of a reply hands `ELEP` to the length
/// parser — which `request_snapshot` recognises and steps over. Under no load
/// the ACK always beat the first window and that path never ran; under a
/// parallel test batch it did, every time.
///
/// This no longer drains anything (see `control_channel_activated`), so a `false`
/// here is a statement about the child and not a stream that has been eaten. The
/// caller may ask again next window without cost.
///
/// Bounded, because a program that never activates must not hang the view: after
/// the deadline the caller proceeds and finds out from the snapshot itself.
pub(crate) fn await_activation(channel: &ControlChannel, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if control_channel_activated(channel) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        // Short enough that a program which boots promptly is not made to look
        // slow, long enough not to spin on a socket that has nothing to say.
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Every line prefix the profiler emits on a program's stderr.
///
/// An exact list, not a pattern. Both mechanisms write to the same stream, so
/// the monitor has to tell its own output from the program's, and the two ways
/// of being wrong are not equal: an unrecognised profiler line reaches the
/// operator as a visible, harmless extra row, while a program's line matched by
/// mistake is DELETED with nothing to say it existed.
///
/// So a new kind of profiler line has to be added here, and until it is, it
/// leaks. That is the direction to fail in — and the reason this is a list a
/// reader can check against `elephc-instr`/`elephc-probe` rather than a rule
/// that quietly covers whatever it happens to cover.
const PROFILER_LINE_PREFIXES: [&str; 9] = [
    "elephc-instr:",
    "elephc-instr-edge:",
    "elephc-instr-query:",
    "elephc-instr-query-dropped:",
    "elephc-instr-trace:",
    "elephc-probe:",
    "elephc-probe-alloc:",
    "elephc-probe-io:",
    "elephc-probe-samples:",
];

/// Whether a line of a monitored program's stderr is the profiler's own output
/// rather than the program's.
///
/// Matched on the whole first token against the list above. A prefix test would
/// swallow a program that writes `elephc-instrumentation disabled` on its own
/// stderr — a line the author wrote, removed by the tool that was supposed to be
/// watching it — and even a prefix-plus-colon test would take
/// `elephc-instr-custom:` from a program that chose that name.
pub(crate) fn is_profiler_line(line: &str) -> bool {
    let Some(first) = line.split_whitespace().next() else {
        return false;
    };
    PROFILER_LINE_PREFIXES.contains(&first)
}

/// Gives a socket a receive deadline, in whole seconds.
///
/// Split out so a test can ask for a short one. The behaviour worth testing is
/// what happens WHEN a deadline passes, and a test that has to wait five real
/// seconds to reach it is a test that gets shortened until it no longer reaches
/// it at all.
pub(crate) fn set_receive_deadline(fd: i32, seconds: i64) {
    let timeout = libc::timeval { tv_sec: seconds, tv_usec: 0 };
    // Safety: setting a documented option on a socket this process owns.
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &timeout as *const libc::timeval as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }
}

/// How many consecutive deadlines a half-delivered reply may miss before the
/// channel is called finished.
///
/// A reply arriving in pieces resets the deadline on every piece, so reaching
/// even one of these means a whole deadline passed with NOTHING arriving
/// mid-message — a writer that has stopped rather than one that is slow. Three,
/// because ending the view reaps the target, and the difference between slow and
/// stopped is worth a few seconds of patience.
const MID_MESSAGE_STALLS: u32 = 3;

/// What asking the child for a snapshot produced.
///
/// Three outcomes and not two, because "no answer" and "no child" call for
/// opposite things. A live view that ends kills the target it was watching — the
/// program only outlives the loop if the loop is still running — so treating a
/// slow answer as a dead one does not merely lose a window: it stops a healthy
/// program the operator was in the middle of profiling.
pub(crate) enum Snapshot {
    /// The child answered.
    Answered(String),
    /// It did not answer in time and nothing OF THE REPLY was consumed, so it
    /// can be asked again on the next window.
    ///
    /// `activation_seen` is true when this call took the child's ACK off the
    /// front before giving up on the reply. The caller has to record that:
    /// otherwise its `activated` flag stays false over an ACK that no longer
    /// exists, and every later window opens by spending the whole activation
    /// deadline waiting for a message nobody will send again.
    Late { activation_seen: bool },
    /// The channel is finished — closed, broken, or desynchronised by a reply
    /// that stopped half way. Nothing further can be asked over it.
    Gone,
}

/// Request byte the probe answers with a snapshot of what it has sampled.
///
/// Mirrors `CONTROL_SNAPSHOT_REQUEST` in `elephc-probe`; the two are one
/// protocol and the constant is named the same on both sides so a `grep` finds
/// the pair.
const CONTROL_SNAPSHOT_REQUEST: u8 = b'S';

/// Asks the monitored child for everything it has sampled so far.
///
/// The channel is the credential: only this process holds the parent end, so a
/// program `monitor` launched can simply be asked. That is the whole reason
/// `--live` no longer needs a tool that reads a process from the outside — and
/// therefore no longer needs macOS.
///
/// Cumulative, like the endpoint's answer. One window is the difference between
/// two of these, which is the caller's arithmetic rather than the probe's: a
/// probe that reset on every read would lose a window to any reader that
/// disconnected, and would have to decide what a window is.
///
/// Three answers, and the caller has to tell them apart. `Answered` carries the
/// child's cumulative text. `Late` is the empty window — a child too young to
/// have a thread reading yet, or one that missed the deadline with nothing
/// consumed; it is asked again on the next one. `Gone` is NOT an empty window:
/// the channel is finished, and the live loop ends the view and reaps the target
/// on it. This paragraph used to describe a `None` returned "on any failure" and
/// read as an empty window — the shape before `Snapshot` existed, and it folded
/// together exactly the two cases the enum was added to separate. Written
/// against it, a caller hangs a live view on a dead child.
pub(crate) fn request_snapshot(channel: &ControlChannel) -> Snapshot {
    let fd = channel.parent;
    let request = [CONTROL_SNAPSHOT_REQUEST];
    // Safety: `fd` is this process's end of the socketpair, owned by `channel`.
    let sent = unsafe {
        libc::send(fd, request.as_ptr() as *const libc::c_void, 1, 0)
    };
    if sent != 1 {
        // `EPIPE` here, with SIGPIPE ignored by the runtime, means the peer is
        // closed: the child is gone.
        return Snapshot::Gone;
    }
    let mut header = [0u8; 4];
    // A timeout on the FIRST word consumed nothing, so the stream is still where
    // it was and the next window can ask again. A timeout after that did not: the
    // reply is half read, and asking again would parse the rest of it as the next
    // answer's length. That is why the two are distinguished here rather than
    // both being called "no answer".
    match recv_exact(fd, &mut header) {
        Read::Filled => {}
        Read::TimedOutClean => return Snapshot::Late { activation_seen: false },
        Read::Ended => return Snapshot::Gone,
    }
    // The activation ACK is sent once, at the child's init, and may still be in
    // the buffer when this reads. `await_activation` is supposed to have taken
    // it — but it has a deadline, and a child that boots slower than that
    // deadline sends its ACK straight into this read instead. Its first four
    // bytes are `ELEP`, which decodes as a 1.3 GB length, which the bound below
    // refuses as `Gone`, which ends a live view on a program that is running
    // perfectly well.
    //
    // So it is handled here rather than only prevented upstream: a wait that can
    // TIME OUT cannot be the only thing standing between the two messages. This
    // holds whatever the child's boot takes.
    if header == CONTROL_ACK[..4] {
        let mut rest = [0u8; CONTROL_ACK.len() - 4];
        // Mid-message now, exactly as in the body loop below: the stream owes
        // these bytes, so a clean timeout is worth retrying rather than
        // abandoning half an ACK for the next window to parse as a length.
        let mut stalls = 0u32;
        loop {
            match recv_exact(fd, &mut rest) {
                Read::Filled => break,
                Read::TimedOutClean if stalls + 1 < MID_MESSAGE_STALLS => stalls += 1,
                Read::TimedOutClean | Read::Ended => return Snapshot::Gone,
            }
        }
        if rest != CONTROL_ACK[4..] {
            return Snapshot::Gone;
        }
        // The reply this call asked for is still on its way. The ACK, though, is
        // consumed and gone — so this `Late` has to carry that out with it.
        match recv_exact(fd, &mut header) {
            Read::Filled => {}
            Read::TimedOutClean => return Snapshot::Late { activation_seen: true },
            Read::Ended => return Snapshot::Gone,
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 {
        return Snapshot::Answered(String::new());
    }
    // Bounded by what a folded profile can be, so a corrupt or hostile length
    // cannot make this allocate the machine away. The probe folds at most the
    // ring, which is orders of magnitude below this.
    const MAX_SNAPSHOT: usize = 64 * 1024 * 1024;
    if len > MAX_SNAPSHOT {
        return Snapshot::Gone;
    }
    let mut body = vec![0u8; len];
    // Being mid-message is a property of the MESSAGE, not of this read. The
    // header is already consumed, so the stream owes exactly `len` bytes — which
    // is what makes waiting longer safe here and nowhere else, whether or not
    // any of the body has arrived yet.
    //
    // That distinction is the whole of it, and the first version of this got it
    // wrong: it retried only once a body byte had landed, so the ordinary shape
    // — header, then a stall — fell straight through to `Gone`, and `Gone` reaps
    // the target.
    let mut stalls = 0u32;
    loop {
        match recv_exact(fd, &mut body) {
            Read::Filled => break,
            Read::TimedOutClean if stalls + 1 < MID_MESSAGE_STALLS => {
                // Nothing was taken, so the next attempt starts where this one
                // did and refills the same buffer from the same place.
                stalls += 1;
            }
            // Out of patience, or a channel that is finished for another reason.
            Read::TimedOutClean | Read::Ended => return Snapshot::Gone,
        }
    }
    match String::from_utf8(body) {
        Ok(text) => Snapshot::Answered(text),
        Err(_) => Snapshot::Gone,
    }
}

/// How a read of a fixed-size field ended.
enum Read {
    Filled,
    /// The deadline passed with NOTHING taken off the stream.
    TimedOutClean,
    /// Closed, broken, or timed out part way — either way, unusable.
    Ended,
}

/// Fills `buf` completely, or reports that it could not.
///
/// A stream socket may deliver a reply in pieces. Treating a short read as the
/// whole answer would truncate a profile mid-line and silently drop the frames
/// that did not arrive.
fn recv_exact(fd: i32, buf: &mut [u8]) -> Read {
    let mut filled = 0usize;
    let mut stalls = 0u32;
    while filled < buf.len() {
        // Safety: writing into `buf`'s own bytes from a socket this process owns.
        let read = unsafe {
            libc::recv(
                fd,
                buf[filled..].as_mut_ptr() as *mut libc::c_void,
                buf.len() - filled,
                0,
            )
        };
        if read < 0 {
            let error = std::io::Error::last_os_error();
            // An interrupted read has consumed nothing; ending here would report
            // a truncated snapshot as a dead channel and stop the live loop with
            // the program still running.
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                // Nothing taken yet: the stream is where it was, so the next
                // window can ask again.
                if filled == 0 {
                    return Read::TimedOutClean;
                }
                // Part way through. Retrying is safe HERE and only here —
                // `filled` says exactly how much of this reply is still owed, so
                // reading on continues the same message rather than parsing the
                // next one's length out of its tail.
                //
                // Given more deadlines rather than one, because each already
                // measures a stall with no bytes at all, and a child that is
                // merely slow deserves better than being cut off mid-sentence.
                // Bounded, because a writer that has genuinely stopped must not
                // hold the view open for as long as it likes.
                stalls += 1;
                if stalls < MID_MESSAGE_STALLS {
                    continue;
                }
                return Read::Ended;
            }
            return Read::Ended;
        }
        if read == 0 {
            return Read::Ended;
        }
        filled += read as usize;
        // Bytes arrived, so the budget above is spent on a writer that STOPPED
        // and not on one that is slow — which is the distinction its own comment
        // draws, and the code did not make. Without this line the stalls only
        // ever accumulate: a child that sends a piece, pauses past the deadline,
        // sends another, pauses again, is cut off on the third pause while it is
        // visibly still writing. `Ended` here means `Gone`, and `Gone` ends the
        // live view and reaps the target — so the reading that kills a healthy
        // program is the one to get wrong last, which is the arbitration
        // `Snapshot` was introduced to make.
        stalls = 0;
    }
    Read::Filled
}

/// Arranges for `channel`'s child end to arrive as `CONTROL_FD` in the spawned
/// process.
///
/// `pre_exec` runs in the forked child between fork and exec, where only
/// async-signal-safe calls are permitted — `dup2` and `close` are. Nothing else
/// happens here for that reason.
pub(crate) fn attach_control_channel(command: &mut process::Command, channel: &ControlChannel) {
    use std::os::unix::process::CommandExt as _;
    let child_fd = channel.child;
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(child_fd, CONTROL_FD) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The duplicate is what the child keeps; clear CLOEXEC so it survives
            // the exec that follows.
            if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Whether `path` is a binary built with `--with-monitoring`.
///
/// Read from the FILE, not from the running process: the whole value of the
/// check is telling someone "this binary cannot answer that question" before
/// anything is launched. Running it and reporting an empty profile would read as
/// "your program is fast", which is the worst possible way to be wrong.
pub(crate) fn carries_monitoring(path: &std::path::Path) -> bool {
    // Regular files only. `fs::read` on a character device never returns —
    // `monitor /dev/zero` read until the machine gave out — and on a directory
    // it fails in a way that used to read as "no marker".
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes
        .windows(MONITORING_MARKER.len())
        .any(|window| window == MONITORING_MARKER)
}

/// Deliberately strict, with no reduced fallback. An external sampler could still
/// produce time shares for an unequipped binary, but shipping that as a silent
/// downgrade means two different things arrive under one command and the reader
/// has to notice which — the exact ambiguity this whole design removes. One
/// answer, or an error naming the fix.
pub(crate) fn require_monitoring(path: &std::path::Path) -> Result<(), String> {
    // Say what is actually wrong. Every read failure used to collapse into
    // "not built with --with-monitoring", so a typo'd path, a directory, or a
    // permission problem all sent the user off to rebuild a binary that was
    // never the issue — an error that confidently names the wrong cause is
    // worse than one that admits it does not know.
    match std::fs::metadata(path) {
        Ok(meta) if !meta.is_file() => {
            return Err(format!(
                "{} is not a file, so there is nothing to run.",
                path.display()
            ));
        }
        Err(error) => {
            return Err(format!("cannot read {}: {error}", path.display()));
        }
        Ok(_) => {}
    }
    if carries_monitoring(path) {
        return Ok(());
    }
    Err(format!(
        "{} was not built with --with-monitoring, so there is nothing to monitor.\n  \
         Rebuild it:  elephc --with-monitoring <source>.php\n  \
         Or point monitor at the source and let it build:  elephc monitor <source>.php",
        path.display()
    ))
}

/// Resolves the build key for `--probe-host`: `ELEPHC_PROBE_KEY` hex if set,
/// else the `<socket-without-.sock>.key` file, else a `.key`
/// next to the socket path.
pub(crate) fn resolve_probe_key(cmd: &MonitorCommand, socket: &str) -> Result<[u8; 32], String> {
    if let Some(path) = &cmd.probe_key {
        let hex = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read probe key {path}: {error}"))?;
        return parse_hex_key(hex.trim())
            .ok_or_else(|| format!("probe key {path} is not 64 hex characters"));
    }
    if let Ok(hex) = std::env::var("ELEPHC_PROBE_KEY") {
        return parse_hex_key(hex.trim())
            .ok_or_else(|| "ELEPHC_PROBE_KEY is not 64 hex characters".to_string());
    }
    let candidates = [
        format!("{}.key", socket.trim_end_matches(".sock")),
        format!("{socket}.key"),
    ];
    for candidate in &candidates {
        if let Ok(hex) = std::fs::read_to_string(candidate) {
            return parse_hex_key(hex.trim()).ok_or_else(|| {
                format!("probe key sidecar {candidate} is not 64 hex characters")
            });
        }
    }
    Err(format!(
        "no build key: pass --key <file>, set ELEPHC_PROBE_KEY, or place a .key \
         file next to {socket}"
    ))
}
