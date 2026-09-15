//! Purpose:
//! Real streaming implementation of PHP's `session.upload_progress` for the
//! `--web` prefork server. Owns the incremental multipart progress tracker, the
//! `php`/`php_serialize`/`php_binary` serializers for the progress array, and
//! the short-lock
//! read-modify-write that splices the progress entry into the session file while
//! a `multipart/form-data` upload is still being received.
//!
//! Called from:
//! - `crate::worker`'s request body-drain path: `begin` decides whether to
//!   track, `Tracker::update` runs per received frame, and `Tracker::complete`
//!   runs once the body is fully drained (before the PHP handler executes).
//!
//! Key details:
//! - Progress writes use independent `open -> flock(LOCK_EX) -> read -> modify
//!   -> write -> close` cycles on `<save_path>/sess_<id>`; they NEVER touch the
//!   handler's persistent `state::SESSION_FD`, so the short lock is never held
//!   across the whole upload. The handler runs after the drain, so all progress
//!   writes are flushed and unlocked before `session_start` locks the file.
//! - The tracker parses INCREMENTALLY: a cursor at the last consumed boundary means each
//!   completed part is parsed exactly once, and the trailing in-flight part's header block
//!   is parsed once per part, after which only its byte count moves. Total work over an
//!   upload is linear in the body size however many frames it arrives in — re-parsing the
//!   whole accumulated buffer after every frame cost frames x body, which an unauthenticated
//!   client could drive with many small frames (issue #885). Boundary and part-header
//!   lengths are bounded, and byte searches use `memchr::memmem` so an attacker-chosen
//!   repeated-prefix boundary cannot force a quadratic scan. It never panics, and it still
//!   degrades gracefully on a truncated or adversarial body.
//! - The RMW preserves unrelated entries under all three registered session
//!   serializers and accepts Cookie, GET, or multipart POST session IDs.

use std::ffi::CStr;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::time::Instant;

use super::file_io::{
    configured_session_file_path, lock_exclusive, open_session_file, parse_save_path,
};

/// Longest multipart boundary this tracker will follow, in bytes.
///
/// RFC 2046 caps a boundary at 70 characters; the value is attacker-controlled and every
/// delimiter search pays its length, so anything longer is refused outright rather than
/// tracked. A refused boundary only turns progress tracking off for that request.
const MAX_BOUNDARY_BYTES: usize = 70;

/// Longest part header block this tracker will scan for its `\r\n\r\n` terminator.
///
/// A body that never sends one would otherwise have its whole growing tail re-scanned on
/// every frame. 8 KiB is far above any real `Content-Disposition`/`Content-Type` pair and
/// matches the order of the header limits the server itself applies.
const MAX_PART_HEADER_BYTES: usize = 8 * 1024;

/// Per-file progress snapshot mirroring one entry of PHP's `files` sub-array.
#[derive(Clone)]
struct FileProgress {
    /// The multipart `name="…"` of the file field.
    field_name: Vec<u8>,
    /// The client-supplied `filename="…"`.
    name: Vec<u8>,
    /// Bytes of this file's content received so far.
    bytes_processed: usize,
    /// Whether this file part has been fully received (closing boundary seen).
    done: bool,
}

/// Which serialize handler frames the session file, decided once at `begin`.
#[derive(Clone, Copy, PartialEq)]
enum Handler {
    /// `key|serialize(value)` entries concatenated (default).
    Php,
    /// The whole `$_SESSION` as a single `serialize()` array.
    PhpSerialize,
    /// `chr(strlen(key)).key.serialize(value)` entries concatenated.
    PhpBinary,
}

/// Live tracker for one in-flight multipart upload. Constructed by [`begin`]
/// only when progress tracking is warranted; otherwise the fast buffer-only
/// drain path runs and no tracker exists.
pub(crate) struct Tracker {
    /// Session save directory (`<save_path>/sess_<id>`).
    save_path: String,
    /// Validated session id from Cookie, query string, or a multipart field.
    sid: Option<String>,
    /// Multipart field name that may carry the POST session id.
    sid_field: Vec<u8>,
    /// `$_SESSION` key prefix (`session.upload_progress.prefix`).
    prefix: String,
    /// Form field name whose value becomes the progress key
    /// (`session.upload_progress.name`).
    name_field: Vec<u8>,
    /// Multipart boundary delimiter (`--<boundary>`), without the trailing CRLF.
    delim: Vec<u8>,
    /// Serialize handler for the RMW.
    handler: Handler,
    /// Whether the entry is removed on completion (`upload_progress.cleanup`).
    cleanup: bool,
    /// Request `Content-Length`, or `-1` when unknown.
    content_length: i64,
    /// Request start time in whole Unix seconds (`start_time`).
    start_time: i64,
    /// Bytes between throttled writes derived from `upload_progress.freq`.
    freq_bytes: usize,
    /// Minimum seconds between throttled writes (`upload_progress.min_freq`).
    min_freq: f64,
    /// Body length at the last write (throttle baseline).
    last_write_bytes: usize,
    /// Wall-clock instant of the last write (throttle baseline).
    last_write_time: Instant,
    /// The progress key (value of the trigger field), once its part is parsed.
    key: Option<Vec<u8>>,
    /// Absolute offset of the LAST boundary delimiter already consumed, or `None`
    /// before the first one is seen.
    ///
    /// THE CURSOR THAT MAKES THE SCAN INCREMENTAL. `update()` is handed the whole
    /// accumulated body after every received frame, and the body is append-only, so a
    /// byte offset stays valid across calls. Re-parsing from zero each time made total
    /// work grow with frames x body — an unauthenticated remote client could send a large
    /// body as many small frames and spend the worker's CPU quadratically (issue #885).
    last_delim: Option<usize>,
    /// How far the delimiter search has already looked, whether or not it found anything.
    ///
    /// `last_delim` alone is not enough to make the scan incremental, and that was the
    /// remaining quadratic window. A frame that finds NO new delimiter leaves `last_delim`
    /// where it was, so the next frame restarted from it and re-searched the whole open
    /// tail — `O(frames x tail)` for as long as one part keeps growing, which is exactly
    /// the pre-trigger window a client controls by delaying the progress-key field.
    ///
    /// Resuming from here instead means every byte is searched once. The search restarts
    /// `delim.len() - 1` bytes earlier so a delimiter straddling two frames is still found:
    /// that is the most of one that can sit in the already-scanned region without having
    /// been matched.
    scanned: usize,
    /// How far the in-flight part's HEADER search has already looked, relative to the part.
    ///
    /// Same rule as `scanned`, for the same reason, bounded by [`MAX_PART_HEADER_BYTES`].
    /// Reset whenever a new delimiter starts a new part.
    inflight_header_scanned: usize,
    /// Completed-part file entries accumulated so far, one per boundary-delimited part.
    completed: Vec<FileProgress>,
    /// The in-flight part after `last_delim`, once its header block has been parsed.
    ///
    /// Parsed ONCE per part rather than on every frame: only `bytes_processed` changes as
    /// the body grows, and that is arithmetic on the header end offset.
    inflight: Option<Inflight>,
    /// Set when the in-flight part's header block exceeded [`MAX_PART_HEADER_BYTES`]
    /// without a terminator, so the tail is not re-scanned on every later frame.
    inflight_unparsable: bool,
    /// Whether `key` came from a trigger field that was still STREAMING.
    ///
    /// Such a key is a prefix of the real one — possibly the empty prefix — so it must be
    /// replaced when the part completes. Latching it permanently is what made a body fed one
    /// byte at a time settle on an empty progress key.
    key_provisional: bool,
}

/// The in-flight part's parsed header block, held across frames.
struct Inflight {
    /// Absolute offset one past the part's `\r\n\r\n` header terminator.
    content_start: usize,
    /// `Content-Disposition` field name.
    field_name: Vec<u8>,
    /// `Content-Disposition` filename, when the part is a file part.
    filename: Option<Vec<u8>>,
}

impl Tracker {
    /// Full `$_SESSION` key (`<prefix><trigger-field-value>`) once the key is
    /// known, else `None`.
    fn full_key(&self) -> Option<Vec<u8>> {
        self.key.as_ref().map(|k| {
            let mut fk = self.prefix.as_bytes().to_vec();
            fk.extend_from_slice(k);
            fk
        })
    }

    /// Feeds the bytes received so far. Extracts the progress key once its
    /// field part is complete, then writes a throttled progress snapshot when
    /// the freq/min_freq thresholds are crossed. Never writes before the key is
    /// known (PHP requires the trigger field before the file parts).
    pub(crate) fn update(&mut self, body: &[u8]) {
        let bytes = body.len();
        let enough_bytes = bytes.saturating_sub(self.last_write_bytes) >= self.freq_bytes;
        let enough_time = self.last_write_time.elapsed().as_secs_f64() >= self.min_freq;
        // THE THROTTLE IS CHECKED FIRST once the trigger field and session id are known, so
        // a frame that cannot produce an observable update does NO work at all — not even
        // the incremental advance. Deferring it is safe and strictly cheaper: the cursor is
        // an absolute offset, so a later advance consumes the skipped bytes exactly once.
        // Before the key and session id are known the advance has to run, because it is what
        // finds them.
        if self.ready_to_write() && !(enough_bytes && enough_time) {
            return;
        }
        self.advance(body);
        if !self.ready_to_write() || !(enough_bytes && enough_time) {
            return;
        }
        let Some(full_key) = self.full_key() else {
            return;
        };
        // The file list is BUILT ONLY HERE, once a write is actually due. Building it on
        // every frame copied every stored part name and filename again, so a client that
        // sent many file parts and then kept sending tiny frames paid parts x frames in
        // allocations — the same shape the incremental cursor removes for parsing.
        let files = self.files(body);
        let value = self.serialize_progress(&files, bytes, false);
        if self.write_entry(&full_key, &value) {
            self.last_write_bytes = bytes;
            self.last_write_time = Instant::now();
        }
    }

    /// Whether a progress entry may be written yet.
    ///
    /// A PROVISIONAL key does not count. It is a prefix of the real trigger-field value, so
    /// writing under it would create a session entry at a key `complete()` never removes:
    /// with `upload_progress.cleanup` on, that stale `done => false` record would outlive
    /// the request. PHP requires the trigger field to be COMPLETE before it tracks anything,
    /// so waiting also matches the interpreter.
    fn ready_to_write(&self) -> bool {
        self.sid.is_some() && self.key.is_some() && !self.key_provisional
    }

    /// Finalizes progress once the body is fully drained: marks every file and
    /// the whole upload `done`, then does one last write — removing the entry
    /// when `cleanup` is on, or persisting the `done => true` snapshot otherwise.
    pub(crate) fn complete(&mut self, body: &[u8]) {
        self.advance(body);
        if !self.ready_to_write() {
            return; // No session id, or no COMPLETE trigger field: nothing to finalize.
        }
        let Some(full_key) = self.full_key() else {
            return;
        };
        let mut files = self.files(body);
        for f in &mut files {
            f.done = true;
        }
        if self.cleanup {
            self.remove_entry(&full_key);
        } else {
            let value = self.serialize_progress(&files, body.len(), true);
            self.write_entry(&full_key, &value);
        }
    }

    /// Advances the incremental parse over whatever bytes are new since the last call.
    ///
    /// INCREMENTAL BY CONSTRUCTION. Completed parts are parsed exactly once, when the
    /// boundary that closes them arrives, and their results accumulate in `completed`. The
    /// trailing in-flight part's header block is parsed once too; afterwards only its byte
    /// count moves. Total work over a whole upload is therefore linear in the body size no
    /// matter how many frames it arrives in — the property issue #885 asks for, where
    /// re-parsing the whole accumulated buffer after every frame cost frames x body.
    ///
    /// The progress key and session id land in `self.key`/`self.sid` as their parts are
    /// consumed: they are set once and must survive the incremental advance. The per-file
    /// list is built separately by [`Tracker::files`], only when a write is due.
    ///
    /// Tolerant of truncation — never panics.
    fn advance(&mut self, body: &[u8]) {
        self.advance_completed_parts(body);
        self.advance_inflight_part(body);
        if let Some(inflight) = &self.inflight {
            if inflight.filename.is_none()
                && (self.key.is_none() || self.key_provisional)
                && inflight.field_name == self.name_field
            {
                // Rare: trigger field still streaming — take what we have, and remember that
                // it is a PREFIX so the completed part replaces it. Nothing is written while
                // the key is provisional; see `ready_to_write`.
                self.key = Some(body[inflight.content_start.min(body.len())..].to_vec());
                self.key_provisional = true;
            }
        }
    }

    /// Builds the per-file list for one progress write: every completed part, plus the
    /// in-flight one with the bytes received so far.
    ///
    /// SEPARATE FROM [`Tracker::advance`] and called ONLY when a write is due. It copies
    /// every stored part name and filename, so running it per frame made the cost grow with
    /// parts x frames for a client that sent many file parts and then kept the connection
    /// trickling.
    fn files(&self, body: &[u8]) -> Vec<FileProgress> {
        let mut files = self.completed.clone();
        if let Some(inflight) = &self.inflight {
            if let Some(name) = &inflight.filename {
                files.push(FileProgress {
                    field_name: inflight.field_name.clone(),
                    name: name.clone(),
                    bytes_processed: body.len().saturating_sub(inflight.content_start),
                    done: false,
                });
            }
        }
        files
    }

/// Where a delimiter search should resume, given what has already been searched.
///
/// This is THE fix for the pre-trigger quadratic window, isolated so it can be asserted on
/// directly: the old behaviour was `part_start` unconditionally, so a frame that found no
/// new delimiter left the next frame re-searching the whole open tail.
///
/// Resuming at `scanned - (delim_len - 1)` searches every byte exactly once while still
/// catching a delimiter straddling two frames — `delim_len - 1` is the most of one that can
/// sit in the already-searched region without having matched. Never before `part_start`,
/// because bytes belonging to an earlier part are not this part's to scan.
fn resume_index(part_start: usize, scanned: usize, delim_len: usize) -> usize {
    part_start.max(scanned.saturating_sub(delim_len.saturating_sub(1)))
}

    /// Consumes every boundary-delimited part that has become complete since the last call.
    ///
    /// The cursor only ever moves forward, and each new delimiter is found with
    /// `memchr::memmem` — a two-way search whose cost is linear in the bytes scanned,
    /// unlike the sliding comparison this replaced, which an adversarial repeated-prefix
    /// boundary drove to `O(body x boundary)`.
    fn advance_completed_parts(&mut self, body: &[u8]) {
        let part_start = match self.last_delim {
            Some(previous) => previous + self.delim.len(),
            None => 0,
        };
        let mut search_from = Self::resume_index(part_start, self.scanned, self.delim.len());
        while search_from <= body.len() {
            let Some(relative) = memchr::memmem::find(&body[search_from..], &self.delim) else {
                break;
            };
            let position = search_from + relative;
            if let Some(previous) = self.last_delim {
                let segment = &body[previous + self.delim.len()..position];
                let segment = strip_prefix(segment, b"\r\n");
                let segment = strip_suffix(segment, b"\r\n");
                self.absorb_completed_part(segment);
            }
            self.last_delim = Some(position);
            // A new part begins: whatever was in flight is now complete or gone, and its
            // header search starts over from the new part's first byte.
            self.inflight = None;
            self.inflight_unparsable = false;
            self.inflight_header_scanned = 0;
            search_from = position + self.delim.len();
        }
        self.scanned = body.len();
    }

    /// Records one completed part: a file entry, the progress key, or the session id.
    fn absorb_completed_part(&mut self, segment: &[u8]) {
        let Some((name, filename, content)) = parse_part(segment) else {
            return;
        };
        if let Some(file_name) = filename {
            self.completed.push(FileProgress {
                field_name: name,
                name: file_name,
                bytes_processed: content.len(),
                done: true,
            });
        } else if (self.key.is_none() || self.key_provisional) && name == self.name_field {
            self.key = Some(content.to_vec());
            self.key_provisional = false;
        } else if self.sid.is_none() && name == self.sid_field {
            let value = String::from_utf8_lossy(content).into_owned();
            if valid_sid(&value) {
                self.sid = Some(value);
            }
        }
    }

    /// Parses the trailing in-flight part's header block, ONCE per part.
    ///
    /// Gives up past [`MAX_PART_HEADER_BYTES`] rather than re-scanning a growing tail on
    /// every frame: a body that never sends a header terminator would otherwise re-scan
    /// everything received so far each time, which is the same quadratic shape the
    /// completed-part cursor removes.
    fn advance_inflight_part(&mut self, body: &[u8]) {
        if self.inflight.is_some() || self.inflight_unparsable {
            return;
        }
        let Some(last) = self.last_delim else {
            return;
        };
        let after_delim = last + self.delim.len();
        if after_delim > body.len() {
            return;
        }
        let raw_tail = &body[after_delim..];
        let tail = strip_prefix(raw_tail, b"\r\n");
        let tail_start = after_delim + (raw_tail.len() - tail.len());
        // A closing boundary marker ("--") means the upload has ended.
        if tail.starts_with(b"--") {
            self.inflight_unparsable = true;
            return;
        }
        let window = tail.len().min(MAX_PART_HEADER_BYTES);
        // Resume the terminator search where it stopped, backing up three bytes so a
        // `\r\n\r\n` split across frames is still found. Without this the header window is
        // re-searched on every frame until it is complete — bounded by 8 KiB rather than by
        // the body, but still restarting from the same index each time.
        let resume = self.inflight_header_scanned.saturating_sub(3).min(window);
        let found = memchr::memmem::find(&tail[resume..window], b"\r\n\r\n").map(|at| resume + at);
        self.inflight_header_scanned = window;
        let Some(header_end) = found else {
            if tail.len() >= MAX_PART_HEADER_BYTES {
                self.inflight_unparsable = true;
            }
            return;
        };
        let Some((name, filename, _)) = parse_part(tail) else {
            self.inflight_unparsable = true;
            return;
        };
        self.inflight = Some(Inflight {
            content_start: tail_start + header_end + 4,
            field_name: name,
            filename,
        });
    }

    /// Serializes the progress array in the active handler's value grammar
    /// (identical for `php` and `php_serialize` — only the file framing differs,
    /// handled in the RMW). Byte layout matches PHP `serialize()`.
    fn serialize_progress(
        &self,
        files: &[FileProgress],
        bytes_processed: usize,
        done: bool,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"a:5:{");
        ser_str(&mut out, b"start_time");
        ser_int(&mut out, self.start_time);
        ser_str(&mut out, b"content_length");
        ser_int(&mut out, self.content_length);
        ser_str(&mut out, b"bytes_processed");
        ser_int(&mut out, bytes_processed as i64);
        ser_str(&mut out, b"done");
        ser_bool(&mut out, done);
        ser_str(&mut out, b"files");
        out.extend_from_slice(format!("a:{}:{{", files.len()).as_bytes());
        for (i, f) in files.iter().enumerate() {
            ser_int(&mut out, i as i64);
            out.extend_from_slice(b"a:7:{");
            ser_str(&mut out, b"field_name");
            ser_str(&mut out, &f.field_name);
            ser_str(&mut out, b"name");
            ser_str(&mut out, &f.name);
            ser_str(&mut out, b"tmp_name");
            ser_str(&mut out, b"");
            ser_str(&mut out, b"error");
            ser_int(&mut out, 0);
            ser_str(&mut out, b"done");
            ser_bool(&mut out, f.done);
            ser_str(&mut out, b"start_time");
            ser_int(&mut out, self.start_time);
            ser_str(&mut out, b"bytes_processed");
            ser_int(&mut out, f.bytes_processed as i64);
            out.extend_from_slice(b"}");
        }
        out.extend_from_slice(b"}}");
        out
    }

    /// Short-lock read-modify-write that sets `full_key = value` in the session
    /// file, preserving every other entry verbatim. Returns true on success.
    fn write_entry(&self, full_key: &[u8], value: &[u8]) -> bool {
        self.rmw(|data| Some(set_entry(data, full_key, value, self.handler)))
    }

    /// Short-lock read-modify-write that removes `full_key` from the session
    /// file, preserving every other entry verbatim. Returns true on success.
    fn remove_entry(&self, full_key: &[u8]) -> bool {
        self.rmw(|data| Some(remove_entry(data, full_key, self.handler)))
    }

    /// Runs `edit` under an independent `open -> flock(LOCK_EX) -> read ->
    /// truncate+write -> unlock -> close` cycle on the session file. Never uses
    /// the handler's persistent fd. Closes every transient fd (no fd leak) and
    /// releases the lock on every path. Returns true when the edit was written.
    fn rmw(&self, edit: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> bool {
        let Some(sid) = self.sid.as_deref() else {
            return false;
        };
        let Some(config) = parse_save_path(&self.save_path) else {
            return false;
        };
        let Some(path) = configured_session_file_path(&self.save_path, sid) else {
            return false;
        };
        let file = match open_session_file(&path, config.mode) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let fd = file.as_raw_fd();
        // SAFETY: fd is valid for the lifetime of `file`; flock is advisory and
        // serializes this transient writer against concurrent poll requests.
        if !lock_exclusive(fd) {
            return false;
        }
        let mut data = Vec::new();
        if (&file).read_to_end(&mut data).is_err() {
            return false;
        }
        let mut ok = false;
        if let Some(new_data) = edit(&data) {
            // SAFETY: truncate then retry `pwrite` until the full buffer is on
            // disk; interrupted and short writes are not success.
            unsafe {
                ok = libc::ftruncate(fd, 0) == 0;
                let mut offset = 0usize;
                while ok && offset < new_data.len() {
                    let wrote = libc::pwrite(
                        fd,
                        new_data[offset..].as_ptr() as *const _,
                        new_data.len() - offset,
                        offset as libc::off_t,
                    );
                    if wrote > 0 {
                        offset += wrote as usize;
                    } else if wrote < 0
                        && std::io::Error::last_os_error().kind()
                            == std::io::ErrorKind::Interrupted
                    {
                        continue;
                    } else {
                        ok = false;
                    }
                }
                ok = ok && libc::fsync(fd) == 0;
            }
        }
        // SAFETY: release the advisory lock before the fd is closed by `drop`.
        unsafe {
            libc::flock(fd, libc::LOCK_UN);
        }
        drop(file); // closes the fd
        ok
    }
}

/// Materializes lazy session configuration before Tokio starts concurrent tasks.
pub(crate) fn initialize_config() {
    unsafe {
        let _ = super::state::elephc_web_session_get_name();
        let _ = super::state::elephc_web_session_get_save_path();
        let _ = super::state::elephc_web_session_get_upload_progress_prefix();
        let _ = super::state::elephc_web_session_get_upload_progress_name();
        let _ = super::state::elephc_web_session_get_upload_progress_freq();
        let _ = super::state::elephc_web_session_get_upload_progress_min_freq();
        let _ = super::state::elephc_web_session_get_serialize_handler();
        let _ = super::state::elephc_web_session_get_upload_progress_enabled();
        let _ = super::state::elephc_web_session_get_upload_progress_cleanup();
        let _ = super::state::elephc_web_session_get_use_only_cookies();
    }
}

/// Decides whether to track upload progress for the current request and, if so,
/// builds a [`Tracker`]. Returns `None` (fast buffer-only drain, zero overhead)
/// unless progress is enabled, the body is a `multipart/form-data` upload with a
/// boundary, and the request supplies a valid session ID through an allowed
/// Cookie, query, or multipart form source.
pub(crate) fn begin(headers: &[(String, String)], query: &str) -> Option<Tracker> {
    if !config_enabled() {
        return None;
    }
    let content_type = header_value(headers, "content-type")?;
    let boundary = extract_boundary(&content_type)?;

    // SAFETY: `worker::serve` materializes lazy config before Tokio starts, so
    // concurrent request tasks only read immutable process-static values here.
    let (name_cookie, save_path, prefix, name_field, freq, min_freq, handler_name) = unsafe {
        (
            getter_string(super::state::elephc_web_session_get_name()),
            getter_string(super::state::elephc_web_session_get_save_path()),
            getter_string(super::state::elephc_web_session_get_upload_progress_prefix()),
            getter_string(super::state::elephc_web_session_get_upload_progress_name()),
            getter_string(super::state::elephc_web_session_get_upload_progress_freq()),
            getter_string(super::state::elephc_web_session_get_upload_progress_min_freq()),
            getter_string(super::state::elephc_web_session_get_serialize_handler()),
        )
    };
    let handler = match handler_name.as_str() {
        "php" | "" => Handler::Php,
        "php_serialize" => Handler::PhpSerialize,
        "php_binary" => Handler::PhpBinary,
        _ => return None,
    };

    let cookie_sid = header_value(headers, "cookie")
        .and_then(|cookie| cookie_value(&cookie, &name_cookie))
        .and_then(|value| percent_decode(&value));
    let allow_non_cookie = !config_use_only_cookies();
    let query_sid = if allow_non_cookie {
        form_value(query, &name_cookie)
    } else {
        None
    };
    let sid = cookie_sid.or(query_sid).filter(|value| valid_sid(value));
    if sid.is_none() && !allow_non_cookie {
        return None;
    }

    let content_length = header_value(headers, "content-length")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(-1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let freq_bytes = parse_freq(&freq, content_length);
    let min_freq = min_freq.trim().parse::<f64>().unwrap_or(1.0).max(0.0);

    let mut delim = b"--".to_vec();
    delim.extend_from_slice(boundary.as_bytes());

    Some(Tracker {
        save_path,
        sid,
        sid_field: name_cookie.into_bytes(),
        prefix,
        name_field: name_field.into_bytes(),
        delim,
        handler,
        cleanup: config_cleanup(),
        content_length,
        start_time: now,
        freq_bytes,
        min_freq,
        last_write_bytes: 0,
        // Force the first threshold check to fire immediately once the key is
        // known by backdating the throttle baseline.
        last_write_time: Instant::now() - std::time::Duration::from_secs(3600),
        key: None,
        last_delim: None,
            scanned: 0,
            inflight_header_scanned: 0,
        completed: Vec::new(),
        inflight: None,
        inflight_unparsable: false,
        key_provisional: false,
    })
}

/// Returns the request-seeded upload-progress enabled flag.
fn config_enabled() -> bool {
    unsafe { super::state::elephc_web_session_get_upload_progress_enabled() == 1 }
}

/// Returns the request-seeded upload-progress cleanup flag.
fn config_cleanup() -> bool {
    unsafe { super::state::elephc_web_session_get_upload_progress_cleanup() == 1 }
}

/// Returns the request-seeded cookie-only policy used before PHP executes.
fn config_use_only_cookies() -> bool {
    unsafe { super::state::elephc_web_session_get_use_only_cookies() == 1 }
}

/// Copies a session getter's C-string return into an owned `String`. The
/// pointer is only valid until the next session call, so we copy immediately.
///
/// # Safety
/// `ptr` must be a valid NUL-terminated pointer from a session getter.
unsafe fn getter_string(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

/// Returns the first request header value whose name matches `name`
/// case-insensitively.
fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Extracts the `<name>=<value>` cookie for `name` from a `Cookie` header,
/// splitting on `;`. Returns `None` when absent.
fn cookie_value(cookie: &str, name: &str) -> Option<String> {
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Extracts and percent-decodes a URL-encoded form/query value by name.
fn form_value(form: &str, name: &str) -> Option<String> {
    form.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded_key = percent_decode(key)?;
        if decoded_key == name {
            percent_decode(value)
        } else {
            None
        }
    })
}

/// Decodes `%HH` and `+` URL encoding, rejecting malformed escape sequences.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hi = *bytes.get(index + 1)?;
                let lo = *bytes.get(index + 2)?;
                out.push((hex_value(hi)? << 4) | hex_value(lo)?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Converts one ASCII hexadecimal digit into its numeric value.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Validates a session id used as a filename component: non-empty, at most 256
/// bytes, characters restricted to `a-zA-Z0-9,-` (PHP's id charset). Blocks
/// path traversal via a hostile cookie.
fn valid_sid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b',' || b == b'-')
}

/// Parses `session.upload_progress.freq` into a byte threshold. `"N%"` is a
/// percentage of `content_length`; a bare `"N"` is an absolute byte count. The
/// result is at least 1 so a valid config always eventually writes.
fn parse_freq(freq: &str, content_length: i64) -> usize {
    let freq = freq.trim();
    if let Some(pct) = freq.strip_suffix('%') {
        let pct = pct.trim().parse::<f64>().unwrap_or(1.0);
        if content_length > 0 {
            return ((pct / 100.0) * content_length as f64).round().max(1.0) as usize;
        }
        return 1;
    }
    freq.parse::<usize>().unwrap_or(1).max(1)
}

/// Extracts and unquotes the `boundary=…` value from a multipart Content-Type.
///
/// A boundary longer than [`MAX_BOUNDARY_BYTES`] is REFUSED rather than tracked: the value
/// is attacker-controlled, and every delimiter search pays its length. RFC 2046 caps a
/// boundary at 70 characters, so nothing a real client sends is turned away — refusing here
/// only drops progress tracking for that request, never the upload itself.
fn extract_boundary(content_type: &str) -> Option<String> {
    if !content_type
        .to_ascii_lowercase()
        .contains("multipart/form-data")
    {
        return None;
    }
    for attr in content_type.split(';') {
        let attr = attr.trim();
        if let Some(rest) = attr
            .strip_prefix("boundary=")
            .or_else(|| attr.strip_prefix("boundary ="))
        {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() && v.len() <= MAX_BOUNDARY_BYTES {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Parses one multipart part's header block into `(name, filename, content)`,
/// reading `name="…"` / `filename="…"` from Content-Disposition. Returns `None`
/// when the part has no complete header block or no `name`.
fn parse_part(seg: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>, &[u8])> {
    let split = find(seg, b"\r\n\r\n")?;
    let header_bytes = &seg[..split];
    let content = &seg[split + 4..];
    let headers = String::from_utf8_lossy(header_bytes);
    let mut name: Option<Vec<u8>> = None;
    let mut filename: Option<Vec<u8>> = None;
    for line in headers.split("\r\n") {
        if line
            .to_ascii_lowercase()
            .starts_with("content-disposition:")
        {
            name = extract_quoted(line, "name=").map(|s| s.into_bytes());
            filename = extract_quoted(line, "filename=").map(|s| s.into_bytes());
        }
    }
    Some((name?, filename, content))
}

/// Extracts a quoted attribute value (`key="value"`) from a header line.
fn extract_quoted(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ── PHP serialize scalar emitters ──

/// Appends `s:<byte-len>:"<bytes>";` for a PHP-serialized string.
fn ser_str(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(format!("s:{}:\"", s.len()).as_bytes());
    out.extend_from_slice(s);
    out.extend_from_slice(b"\";");
}

/// Appends `i:<n>;` for a PHP-serialized integer.
fn ser_int(out: &mut Vec<u8>, n: i64) {
    out.extend_from_slice(format!("i:{n};").as_bytes());
}

/// Appends `b:0;`/`b:1;` for a PHP-serialized boolean.
fn ser_bool(out: &mut Vec<u8>, v: bool) {
    out.extend_from_slice(if v { b"b:1;" } else { b"b:0;" });
}

// ── Raw session-file read-modify-write (byte-level, entry-preserving) ──

/// Sets `full_key = value` in a session-file buffer for the given handler,
/// preserving all other entries verbatim.
fn set_entry(data: &[u8], full_key: &[u8], value: &[u8], handler: Handler) -> Vec<u8> {
    match handler {
        Handler::Php => set_entry_php(data, full_key, value),
        Handler::PhpSerialize => set_entry_php_serialize(data, full_key, value),
        Handler::PhpBinary => set_entry_php_binary(data, full_key, value),
    }
}

/// Removes `full_key` from a session-file buffer for the given handler,
/// preserving all other entries verbatim.
fn remove_entry(data: &[u8], full_key: &[u8], handler: Handler) -> Vec<u8> {
    match handler {
        Handler::Php => remove_entry_php(data, full_key),
        Handler::PhpSerialize => remove_entry_php_serialize(data, full_key),
        Handler::PhpBinary => remove_entry_php_binary(data, full_key),
    }
}

/// Walks the `php_binary` format into entry byte ranges and key slices.
fn walk_php_binary_entries(data: &[u8]) -> Vec<(usize, usize, &[u8])> {
    let mut entries = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let key_len = data[position] as usize;
        let key_start = position + 1;
        let key_end = key_start.saturating_add(key_len);
        if key_end > data.len() {
            break;
        }
        let value_end = skip_value(data, key_end);
        if value_end == key_end {
            break;
        }
        entries.push((position, value_end, &data[key_start..key_end]));
        position = value_end;
    }
    entries
}

/// Replaces or appends one `php_binary` entry; keys longer than 127 are ignored.
fn set_entry_php_binary(data: &[u8], full_key: &[u8], value: &[u8]) -> Vec<u8> {
    if full_key.len() > 127 {
        return data.to_vec();
    }
    let mut entry = Vec::with_capacity(1 + full_key.len() + value.len());
    entry.push(full_key.len() as u8);
    entry.extend_from_slice(full_key);
    entry.extend_from_slice(value);
    if let Some(&(start, end, _)) = walk_php_binary_entries(data)
        .iter()
        .find(|(_, _, key)| *key == full_key)
    {
        let mut out = Vec::with_capacity(data.len() + entry.len());
        out.extend_from_slice(&data[..start]);
        out.extend_from_slice(&entry);
        out.extend_from_slice(&data[end..]);
        out
    } else {
        let mut out = data.to_vec();
        out.extend_from_slice(&entry);
        out
    }
}

/// Removes one `php_binary` entry while preserving all other bytes.
fn remove_entry_php_binary(data: &[u8], full_key: &[u8]) -> Vec<u8> {
    if let Some(&(start, end, _)) = walk_php_binary_entries(data)
        .iter()
        .find(|(_, _, key)| *key == full_key)
    {
        let mut out = Vec::with_capacity(data.len());
        out.extend_from_slice(&data[..start]);
        out.extend_from_slice(&data[end..]);
        out
    } else {
        data.to_vec()
    }
}

/// Walks the `php` handler format (`key|serialize(value)` concatenated) into
/// `(entry_start, value_end, key_slice)` tuples. Stops at the first entry it
/// cannot parse, leaving the unparsed tail untouched by the caller.
fn walk_php_entries(data: &[u8]) -> Vec<(usize, usize, &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let mut key_end = pos;
        while key_end < data.len() && data[key_end] != b'|' {
            key_end += 1;
        }
        if key_end >= data.len() {
            break;
        }
        let key = &data[pos..key_end];
        let val_start = key_end + 1;
        let val_end = skip_value(data, val_start);
        if val_end == val_start {
            break;
        }
        out.push((pos, val_end, key));
        pos = val_end;
    }
    out
}

/// `php` handler set: splice `key|value` over the existing entry, or append it.
fn set_entry_php(data: &[u8], full_key: &[u8], value: &[u8]) -> Vec<u8> {
    let entries = walk_php_entries(data);
    let mut entry = Vec::with_capacity(full_key.len() + 1 + value.len());
    entry.extend_from_slice(full_key);
    entry.push(b'|');
    entry.extend_from_slice(value);
    if let Some(&(start, end, _)) = entries.iter().find(|(_, _, k)| *k == full_key) {
        let mut out = Vec::with_capacity(data.len() + entry.len());
        out.extend_from_slice(&data[..start]);
        out.extend_from_slice(&entry);
        out.extend_from_slice(&data[end..]);
        out
    } else {
        let mut out = data.to_vec();
        out.extend_from_slice(&entry);
        out
    }
}

/// `php` handler remove: cut out the `key|value` entry, or return data unchanged.
fn remove_entry_php(data: &[u8], full_key: &[u8]) -> Vec<u8> {
    let entries = walk_php_entries(data);
    if let Some(&(start, end, _)) = entries.iter().find(|(_, _, k)| *k == full_key) {
        let mut out = Vec::with_capacity(data.len());
        out.extend_from_slice(&data[..start]);
        out.extend_from_slice(&data[end..]);
        out
    } else {
        data.to_vec()
    }
}

/// Parses a `php_serialize` top-level array (`a:N:{ key value … }`) into the
/// raw `(key_bytes, value_bytes)` serialized pairs. Returns `None` when the
/// buffer is empty or not a clean top-level array.
fn parse_php_serialize_pairs(data: &[u8]) -> Option<Vec<(&[u8], &[u8])>> {
    if !data.starts_with(b"a:") {
        return None;
    }
    let mut p = 2;
    let cnt_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let count: usize = std::str::from_utf8(&data[cnt_start..p])
        .ok()?
        .parse()
        .ok()?;
    if data.get(p) != Some(&b':') {
        return None;
    }
    p += 1;
    if data.get(p) != Some(&b'{') {
        return None;
    }
    p += 1;
    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let k_start = p;
        let k_end = skip_value(data, k_start);
        if k_end == k_start {
            return None;
        }
        let v_end = skip_value(data, k_end);
        if v_end == k_end {
            return None;
        }
        pairs.push((&data[k_start..k_end], &data[k_end..v_end]));
        p = v_end;
    }
    Some(pairs)
}

/// Re-emits a `php_serialize` top-level array from raw serialized pairs.
fn emit_php_serialize(pairs: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = format!("a:{}:{{", pairs.len()).into_bytes();
    for (k, v) in pairs {
        out.extend_from_slice(k);
        out.extend_from_slice(v);
    }
    out.push(b'}');
    out
}

/// A serialized string key (`s:len:"key";`) for a `php_serialize` array pair.
fn serialize_string_key(key: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    ser_str(&mut out, key);
    out
}

/// `php_serialize` set: replace or append the `full_key` pair inside the
/// top-level array, preserving every other pair verbatim.
fn set_entry_php_serialize(data: &[u8], full_key: &[u8], value: &[u8]) -> Vec<u8> {
    let key_ser = serialize_string_key(full_key);
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = match parse_php_serialize_pairs(data) {
        Some(p) => p
            .into_iter()
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect(),
        None => Vec::new(),
    };
    if let Some(slot) = pairs
        .iter_mut()
        .find(|(k, _)| k.as_slice() == key_ser.as_slice())
    {
        slot.1 = value.to_vec();
    } else {
        pairs.push((key_ser, value.to_vec()));
    }
    emit_php_serialize(&pairs)
}

/// `php_serialize` remove: drop the `full_key` pair from the top-level array.
fn remove_entry_php_serialize(data: &[u8], full_key: &[u8]) -> Vec<u8> {
    let key_ser = serialize_string_key(full_key);
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = match parse_php_serialize_pairs(data) {
        Some(p) => p
            .into_iter()
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect(),
        None => return data.to_vec(),
    };
    pairs.retain(|(k, _)| k.as_slice() != key_ser.as_slice());
    emit_php_serialize(&pairs)
}

/// Skips one complete PHP serialized value at `pos`, returning the position
/// immediately after it (or `pos` on invalid/truncated input). Understands
/// `N b i d s a O C`; recurses for arrays/objects. Never panics.
fn skip_value(data: &[u8], pos: usize) -> usize {
    if pos >= data.len() {
        return pos;
    }
    match data[pos] {
        b'N' => {
            if data.get(pos + 1) == Some(&b';') {
                pos + 2
            } else {
                pos
            }
        }
        b'b' => scan_scalar(data, pos),
        b'i' => scan_scalar(data, pos),
        b'd' => scan_scalar(data, pos),
        b's' => skip_string(data, pos),
        b'a' => skip_collection(data, pos, false),
        b'O' => skip_collection(data, pos, true),
        b'C' => skip_custom(data, pos),
        _ => pos,
    }
}

/// Skips a `X:...;` scalar (`b`/`i`/`d`) whose body runs to the first `;`.
fn scan_scalar(data: &[u8], pos: usize) -> usize {
    let mut p = pos + 1;
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    while p < data.len() && data[p] != b';' {
        p += 1;
    }
    if data.get(p) == Some(&b';') {
        p + 1
    } else {
        pos
    }
}

/// Skips `s:<len>:"<bytes>";`, honoring the declared byte length.
fn skip_string(data: &[u8], pos: usize) -> usize {
    let mut p = pos + 1;
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    let len_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let Some(slen) = std::str::from_utf8(&data[len_start..p])
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return pos;
    };
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    if data.get(p) != Some(&b'"') {
        return pos;
    }
    p += 1;
    if p + slen > data.len() {
        return pos;
    }
    p += slen;
    if data.get(p) != Some(&b'"') {
        return pos;
    }
    p += 1;
    if data.get(p) == Some(&b';') {
        p + 1
    } else {
        pos
    }
}

/// Skips `a:<count>:{…}` and `O:<len>:"name":<count>:{…}` collections by
/// recursively skipping `count*2` inner values.
fn skip_collection(data: &[u8], pos: usize, is_object: bool) -> usize {
    let mut p = pos + 1;
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    if is_object {
        // O:<namelen>:"<name>":
        let nl_start = p;
        while p < data.len() && data[p].is_ascii_digit() {
            p += 1;
        }
        let Some(namelen) = std::str::from_utf8(&data[nl_start..p])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
        else {
            return pos;
        };
        if data.get(p) != Some(&b':') {
            return pos;
        }
        p += 1;
        if data.get(p) != Some(&b'"') {
            return pos;
        }
        p += 1;
        if p + namelen > data.len() {
            return pos;
        }
        p += namelen;
        if data.get(p) != Some(&b'"') {
            return pos;
        }
        p += 1;
        if data.get(p) != Some(&b':') {
            return pos;
        }
        p += 1;
    }
    let cnt_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let Some(count) = std::str::from_utf8(&data[cnt_start..p])
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return pos;
    };
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    if data.get(p) != Some(&b'{') {
        return pos;
    }
    p += 1;
    for _ in 0..count * 2 {
        let next = skip_value(data, p);
        if next == p {
            return pos;
        }
        p = next;
    }
    if data.get(p) == Some(&b'}') {
        p + 1
    } else {
        pos
    }
}

/// Skips `C:<namelen>:"<name>":<datalen>:{<data>}` custom-serialized objects.
fn skip_custom(data: &[u8], pos: usize) -> usize {
    let mut p = pos + 1;
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    let nl_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let Some(namelen) = std::str::from_utf8(&data[nl_start..p])
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return pos;
    };
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    if data.get(p) != Some(&b'"') {
        return pos;
    }
    p += 1;
    if p + namelen > data.len() {
        return pos;
    }
    p += namelen;
    if data.get(p) != Some(&b'"') {
        return pos;
    }
    p += 1;
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    let dl_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let Some(datalen) = std::str::from_utf8(&data[dl_start..p])
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return pos;
    };
    if data.get(p) != Some(&b':') {
        return pos;
    }
    p += 1;
    if data.get(p) != Some(&b'{') {
        return pos;
    }
    p += 1;
    if p + datalen > data.len() {
        return pos;
    }
    p += datalen;
    if data.get(p) == Some(&b'}') {
        p + 1
    } else {
        pos
    }
}

// ── Byte-search helpers ──

/// Returns the offset of the first occurrence of `needle` in `haystack`.
///
/// `memchr::memmem` rather than a sliding comparison, matching `crate::multipart`'s own
/// search: the needles here (the boundary delimiter, a header terminator) are
/// attacker-controlled, and the naive form this replaced degraded to `O(haystack x needle)`
/// on a repeated-prefix boundary (issue #885).
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    memchr::memmem::find(haystack, needle)
}

/// Removes a leading `prefix` from `data` if present.
fn strip_prefix<'a>(data: &'a [u8], prefix: &[u8]) -> &'a [u8] {
    data.strip_prefix(prefix).unwrap_or(data)
}

/// Removes a trailing `suffix` from `data` if present.
fn strip_suffix<'a>(data: &'a [u8], suffix: &[u8]) -> &'a [u8] {
    data.strip_suffix(suffix).unwrap_or(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `php`-handler serializer must match PHP `serialize()` byte-for-byte
    /// for the progress array shape (verified against `php -r`).
    #[test]
    fn serializer_matches_php() {
        let files = vec![FileProgress {
            field_name: b"file1".to_vec(),
            name: b"foo.avi".to_vec(),
            bytes_processed: 68767,
            done: false,
        }];
        // Build a tracker with fixed fields (only the serializer is exercised).
        let t = Tracker {
            save_path: String::new(),
            sid: None,
            sid_field: b"PHPSESSID".to_vec(),
            prefix: String::new(),
            name_field: Vec::new(),
            delim: Vec::new(),
            handler: Handler::Php,
            cleanup: true,
            content_length: 57343257,
            start_time: 1234567890,
            freq_bytes: 1,
            min_freq: 0.0,
            last_write_bytes: 0,
            last_write_time: Instant::now(),
            key: None,
            last_delim: None,
            scanned: 0,
            inflight_header_scanned: 0,
            completed: Vec::new(),
            inflight: None,
            inflight_unparsable: false,
            key_provisional: false,
        };
        let out = t.serialize_progress(&files, 453489, false);
        let expected = b"a:5:{s:10:\"start_time\";i:1234567890;s:14:\"content_length\";i:57343257;s:15:\"bytes_processed\";i:453489;s:4:\"done\";b:0;s:5:\"files\";a:1:{i:0;a:7:{s:10:\"field_name\";s:5:\"file1\";s:4:\"name\";s:7:\"foo.avi\";s:8:\"tmp_name\";s:0:\"\";s:5:\"error\";i:0;s:4:\"done\";b:0;s:10:\"start_time\";i:1234567890;s:15:\"bytes_processed\";i:68767;}}}";
        assert_eq!(out, expected.to_vec());
    }

    /// `php`-handler RMW: inserting the progress key preserves other entries
    /// verbatim, replacing preserves them, and removing preserves them.
    #[test]
    fn php_rmw_preserves_other_entries() {
        let base = b"a|i:1;b|s:3:\"xyz\";".to_vec();
        // Insert new key K.
        let inserted = set_entry_php(&base, b"K", b"i:9;");
        assert_eq!(inserted, b"a|i:1;b|s:3:\"xyz\";K|i:9;".to_vec());
        // Replace K's value; a and b stay byte-identical.
        let replaced = set_entry_php(&inserted, b"K", b"i:42;");
        assert_eq!(replaced, b"a|i:1;b|s:3:\"xyz\";K|i:42;".to_vec());
        // Remove K; a and b remain verbatim.
        let removed = remove_entry_php(&replaced, b"K");
        assert_eq!(removed, base);
    }

    /// Verifies upload-progress edits support `php_binary` framing.
    #[test]
    fn php_binary_rmw_preserves_other_entries() {
        let mut original = vec![4];
        original.extend_from_slice(b"keepi:7;");
        let inserted = set_entry_php_binary(&original, b"upload_key", b"b:1;");
        assert!(inserted.starts_with(&original));
        let entries = walk_php_binary_entries(&inserted);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].2, b"keep");
        assert_eq!(entries[1].2, b"upload_key");
        assert_eq!(remove_entry_php_binary(&inserted, b"upload_key"), original);
    }

    /// `php`-handler RMW replaces an entry sitting in the MIDDLE of the file
    /// without disturbing its neighbours.
    #[test]
    fn php_rmw_replace_middle_entry() {
        let base = b"a|i:1;K|i:5;b|s:1:\"z\";".to_vec();
        let replaced = set_entry_php(&base, b"K", b"i:99;");
        assert_eq!(replaced, b"a|i:1;K|i:99;b|s:1:\"z\";".to_vec());
        let removed = remove_entry_php(&replaced, b"K");
        assert_eq!(removed, b"a|i:1;b|s:1:\"z\";".to_vec());
    }

    /// `php_serialize`-handler RMW round-trips insert/replace/remove within the
    /// top-level array while keeping other pairs verbatim and the count correct.
    #[test]
    fn php_serialize_rmw_round_trip() {
        // $_SESSION = ["a" => 1]
        let base = b"a:1:{s:1:\"a\";i:1;}".to_vec();
        let inserted = set_entry_php_serialize(&base, b"K", b"i:9;");
        assert_eq!(inserted, b"a:2:{s:1:\"a\";i:1;s:1:\"K\";i:9;}".to_vec());
        let replaced = set_entry_php_serialize(&inserted, b"K", b"i:42;");
        assert_eq!(replaced, b"a:2:{s:1:\"a\";i:1;s:1:\"K\";i:42;}".to_vec());
        let removed = remove_entry_php_serialize(&replaced, b"K");
        assert_eq!(removed, base);
    }

    /// The freq parser handles both percentage and absolute forms, flooring at 1.
    #[test]
    fn freq_parsing() {
        assert_eq!(parse_freq("1%", 1000), 10);
        assert_eq!(parse_freq("50%", 200), 100);
        assert_eq!(parse_freq("1%", -1), 1); // unknown content length
        assert_eq!(parse_freq("4096", 100000), 4096);
        assert_eq!(parse_freq("0", 100), 1); // floored to 1
    }

    /// The cookie parser extracts the named session id from a multi-cookie header.
    #[test]
    fn cookie_extraction() {
        assert_eq!(
            cookie_value("foo=1; PHPSESSID=abc123; bar=2", "PHPSESSID").as_deref(),
            Some("abc123")
        );
        assert_eq!(cookie_value("foo=1", "PHPSESSID"), None);
    }

    /// Session-id validation rejects path-traversal and empty ids.
    #[test]
    fn sid_validation() {
        assert!(valid_sid("abc123ABC-,"));
        assert!(!valid_sid(""));
        assert!(!valid_sid("../etc/passwd"));
        assert!(!valid_sid("a/b"));
    }

    /// Verifies Cookie/query SID decoding follows URL form encoding rules.
    #[test]
    fn non_cookie_sid_sources_are_percent_decoded() {
        assert_eq!(percent_decode("ab%2Ccd"), Some("ab,cd".to_string()));
        assert_eq!(form_value("x=1&PHPSESSID=ab%2Ccd", "PHPSESSID"), Some("ab,cd".to_string()));
        assert_eq!(percent_decode("bad%2"), None);
    }

    /// The incremental snapshot extracts the progress key from a completed
    /// trigger field and reports a `done=false` in-flight file for a part whose
    /// closing boundary has not yet arrived.
    #[test]
    fn snapshot_key_and_inflight_file() {
        let mut delim = b"--".to_vec();
        delim.extend_from_slice(b"BOUND");
        let mut t = Tracker {
            save_path: String::new(),
            sid: None,
            sid_field: b"PHPSESSID".to_vec(),
            prefix: "up_".to_string(),
            name_field: b"PHP_SESSION_UPLOAD_PROGRESS".to_vec(),
            delim,
            handler: Handler::Php,
            cleanup: false,
            content_length: 100,
            start_time: 0,
            freq_bytes: 1,
            min_freq: 0.0,
            last_write_bytes: 0,
            last_write_time: Instant::now(),
            key: None,
            last_delim: None,
            scanned: 0,
            inflight_header_scanned: 0,
            completed: Vec::new(),
            inflight: None,
            inflight_unparsable: false,
            key_provisional: false,
        };
        // Completed trigger field, then a file part still streaming (no closing
        // boundary yet).
        let body = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nmykey\r\n--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nPARTIAL";
        t.advance(body);
        let files = t.files(body);
        assert_eq!(t.key.as_deref(), Some(&b"mykey"[..]));
        assert_eq!(t.sid, None);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].field_name, b"f");
        assert_eq!(files[0].name, b"x.bin");
        assert!(!files[0].done, "in-flight file must be done=false");
        assert_eq!(files[0].bytes_processed, b"PARTIAL".len());
    }

    /// Verifies an earlier multipart field can provide the POST session ID.
    #[test]
    fn snapshot_extracts_multipart_session_id() {
        let mut delim = b"--".to_vec();
        delim.extend_from_slice(b"BOUND");
        let mut tracker = Tracker {
            save_path: String::new(),
            sid: None,
            sid_field: b"PHPSESSID".to_vec(),
            prefix: "upload_progress_".to_string(),
            name_field: b"PHP_SESSION_UPLOAD_PROGRESS".to_vec(),
            delim,
            handler: Handler::Php,
            cleanup: true,
            content_length: 0,
            start_time: 0,
            freq_bytes: 1,
            min_freq: 0.0,
            last_write_bytes: 0,
            last_write_time: Instant::now(),
            key: None,
            last_delim: None,
            scanned: 0,
            inflight_header_scanned: 0,
            completed: Vec::new(),
            inflight: None,
            inflight_unparsable: false,
            key_provisional: false,
        };
        let body = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHPSESSID\"\r\n\r\nsid123\r\n--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nkey\r\n--BOUND--\r\n";
        tracker.advance(body);
        let files = tracker.files(body);
        assert_eq!(tracker.key.as_deref(), Some(&b"key"[..]));
        assert_eq!(tracker.sid.as_deref(), Some("sid123"));
        assert!(files.is_empty());
    }

    /// Builds a tracker over the `--BOUND` delimiter for the incremental tests below.
    fn incremental_tracker() -> Tracker {
        let mut delim = b"--".to_vec();
        delim.extend_from_slice(b"BOUND");
        Tracker {
            save_path: String::new(),
            sid: None,
            sid_field: b"PHPSESSID".to_vec(),
            prefix: "up_".to_string(),
            name_field: b"PHP_SESSION_UPLOAD_PROGRESS".to_vec(),
            delim,
            handler: Handler::Php,
            cleanup: false,
            content_length: 100,
            start_time: 0,
            freq_bytes: 1,
            min_freq: 0.0,
            last_write_bytes: 0,
            last_write_time: Instant::now(),
            key: None,
            last_delim: None,
            scanned: 0,
            inflight_header_scanned: 0,
            completed: Vec::new(),
            inflight: None,
            inflight_unparsable: false,
            key_provisional: false,
        }
    }

    /// Issue #885: feeding the body one byte at a time must produce the SAME snapshot as
    /// feeding it whole.
    ///
    /// The tracker is handed the growing accumulated buffer after every frame, so the
    /// incremental cursor has to reach exactly the state a single full parse would. This is
    /// the correctness half of making the scan incremental; the cost half is below.
    #[test]
    fn incremental_feeding_matches_a_single_full_parse() {
        let body: &[u8] = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHPSESSID\"\r\n\r\nsid123\r\n--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nmykey\r\n--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\r\nPARTIAL";

        let mut whole = incremental_tracker();
        whole.advance(body);
        let whole_files = whole.files(body);

        let mut streamed = incremental_tracker();
        let mut streamed_files = Vec::new();
        for end in 1..=body.len() {
            streamed.advance(&body[..end]);
            streamed_files = streamed.files(&body[..end]);
        }

        assert_eq!(streamed.key, whole.key);
        assert_eq!(streamed.sid, whole.sid);
        assert_eq!(streamed_files.len(), whole_files.len());
        for (streamed, whole) in streamed_files.iter().zip(whole_files.iter()) {
            assert_eq!(streamed.field_name, whole.field_name);
            assert_eq!(streamed.name, whole.name);
            assert_eq!(streamed.bytes_processed, whole.bytes_processed);
            assert_eq!(streamed.done, whole.done);
        }
    }

    /// Issue #885: a completed part is parsed EXACTLY ONCE, no matter how many frames arrive
    /// after it.
    ///
    /// The accumulated `completed` list is the observable proof: re-parsing the whole buffer
    /// on every frame would push the same file entry again for each one. Before the fix this
    /// list did not exist and the quadratic re-parse was invisible to any assertion — which
    /// is why the issue asks for a regression test in this module rather than relying on the
    /// `multipart.rs` guard.
    #[test]
    fn a_completed_part_is_absorbed_once_across_many_frames() {
        let head: &[u8] = b"--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\r\nDATA\r\n--BOUND\r\nContent-Disposition: form-data; name=\"g\"; filename=\"y.bin\"\r\n\r\n";
        let mut tracker = incremental_tracker();
        let mut body = head.to_vec();
        tracker.advance(&body);
        assert_eq!(tracker.completed.len(), 1, "first part absorbed once");
        for _ in 0..200 {
            body.extend_from_slice(b"0123456789");
            tracker.advance(&body);
        }
        assert_eq!(
            tracker.completed.len(),
            1,
            "the completed part must not be re-absorbed by later frames"
        );
        let files = tracker.files(&body);
        assert_eq!(files.len(), 2, "one completed file plus the in-flight one");
        assert!(files[0].done);
        assert!(!files[1].done);
        assert_eq!(files[1].bytes_processed, 2000);
    }

    /// Issue #885, the PRE-TRIGGER window: the delimiter search must not restart from
    /// `last_delim` on a frame that finds nothing.
    ///
    /// `last_delim` alone only made the scan incremental ACROSS parts. While one part keeps
    /// growing — a large first part, or a client that simply delays the progress-key field —
    /// no frame finds a new delimiter, so every frame re-searched the whole open tail and
    /// total work stayed `O(frames x tail)`. That window is entirely client-controlled,
    /// which is what makes it the same attack #885 is about.
    ///
    /// Asserted on `resume_index` itself rather than on elapsed time: it is the decision
    /// that was wrong, and a wall-clock assertion would be flaky on a loaded machine. The
    /// pre-fix behaviour was `part_start` unconditionally — the first `assert_ne!` is
    /// exactly what that produced.
    #[test]
    fn the_delimiter_search_resumes_instead_of_restarting() {
        let delim_len = "--BOUND".len();
        let part_start = 64;

        // A part that has grown to 100 KiB without a new delimiter: the next frame must
        // look near the end, not back at the part's first byte.
        let scanned = 64 + 100 * 1024;
        let resume = Tracker::resume_index(part_start, scanned, delim_len);
        assert_ne!(
            resume, part_start,
            "restarting at the part start is the quadratic behaviour this replaced"
        );
        assert_eq!(resume, scanned - (delim_len - 1));

        // Total work over many frames is the body plus a fixed overlap per frame, not the
        // sum of the tails. 500 frames of 10 bytes: linear is ~8 KiB of re-scan, the
        // restart shape is over a megabyte.
        let mut searched = 0usize;
        let mut scanned = part_start;
        for _ in 0..500 {
            let body_len = scanned + 10;
            searched += body_len - Tracker::resume_index(part_start, scanned, delim_len);
            scanned = body_len;
        }
        let linear_bound = (scanned - part_start) + 500 * (delim_len - 1 + 10);
        assert!(
            searched <= linear_bound,
            "search did {searched} bytes of work for {} bytes of body; bound {linear_bound}",
            scanned - part_start
        );

        // The clamp: a cursor from an earlier part never drags the search backwards.
        assert_eq!(Tracker::resume_index(part_start, 0, delim_len), part_start);
        assert_eq!(Tracker::resume_index(part_start, 10, delim_len), part_start);
    }

    /// A delimiter split across two frames is still found once the scan resumes from
    /// `scanned - (delim.len() - 1)` rather than from the frame boundary itself.
    ///
    /// This is the case the overlap exists for, and the one a naive "search only the new
    /// bytes" cursor gets wrong: the frame that completes the delimiter would never see its
    /// leading bytes again.
    #[test]
    fn a_delimiter_split_across_frames_is_still_found() {
        let mut tracker = incremental_tracker();
        let head: &[u8] = b"--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\r\nDATA\r\n";
        let mut body = head.to_vec();
        tracker.advance(&body);
        assert!(tracker.completed.is_empty());

        // Feed the next delimiter one byte at a time: every intermediate frame ends with a
        // partial "--BOUND" that must not be lost.
        for byte in b"--BOUND" {
            body.push(*byte);
            tracker.advance(&body);
        }
        assert_eq!(
            tracker.completed.len(),
            1,
            "the split delimiter closed the first part"
        );
        assert_eq!(tracker.completed[0].name, b"x.bin".to_vec());
    }

    /// Issue #885, raised in review: the same property through the REAL `advance()`, driven
    /// by many small frames on a body whose progress key is never sent.
    ///
    /// `the_delimiter_search_resumes_instead_of_restarting` asserts the decision in
    /// isolation; this one proves the decision is the one `advance()` actually takes in the
    /// window a client controls. The trigger field never arrives, so `key` stays `None`,
    /// `ready_to_write()` stays false, and every frame still runs the full advance — which
    /// is precisely the shape that used to be `O(frames x tail)`.
    ///
    /// The work each frame does is the body length minus the cursor it resumes from, so
    /// summing that over the run is the total scanning work. Linear means "the body, plus a
    /// FIXED overlap per frame"; the restart shape is the sum of the tails, which for these
    /// numbers is two orders of magnitude larger. Both bounds are asserted, so the test
    /// fails whether the cursor stops advancing or merely lags.
    #[test]
    fn many_small_frames_before_the_trigger_stay_linear_in_total_bytes() {
        let mut tracker = incremental_tracker();
        let delim_len = tracker.delim.len();
        // One file part, no `PHP_SESSION_UPLOAD_PROGRESS` field anywhere: the key is never
        // known, so nothing short-circuits the per-frame advance.
        let mut body =
            b"--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\r\n"
                .to_vec();
        let part_start = body.len();
        tracker.advance(&body);

        const FRAMES: usize = 400;
        const FRAME_BYTES: usize = 16;
        let mut searched = 0usize;
        let mut previous_scanned = tracker.scanned;
        for _ in 0..FRAMES {
            // The same two inputs `advance_completed_parts` computes for itself, so the
            // width measured here is the width the search really covers.
            let open_part_start = tracker.last_delim.map_or(0, |d| d + delim_len);
            let resume = Tracker::resume_index(open_part_start, tracker.scanned, delim_len);
            body.extend_from_slice(&[b'Z'; FRAME_BYTES]);
            searched += body.len().saturating_sub(resume);
            tracker.advance(&body);

            assert!(
                tracker.scanned >= previous_scanned,
                "the scan cursor must never go backwards"
            );
            assert!(
                tracker.scanned + delim_len >= body.len(),
                "the cursor must track the body end, or the next frame re-searches the tail: \
                 scanned {} for a {}-byte body",
                tracker.scanned,
                body.len()
            );
            previous_scanned = tracker.scanned;
        }

        assert!(tracker.key.is_none(), "the trigger field never arrived");
        assert!(!tracker.ready_to_write(), "and so no write is authorized");

        let body_bytes = body.len() - part_start;
        let linear_bound = body.len() + FRAMES * (FRAME_BYTES + delim_len);
        let restart_shape = FRAMES * body_bytes / 2;
        assert!(
            searched <= linear_bound,
            "scanned {searched} bytes for {body_bytes} bytes of body; linear bound {linear_bound}"
        );
        assert!(
            linear_bound < restart_shape,
            "the bound must actually separate the two shapes ({linear_bound} vs {restart_shape})"
        );
    }

    /// Issue #885: the header block of the in-flight part is parsed once, and an unbounded
    /// header is abandoned rather than re-scanned on every frame.
    #[test]
    fn an_unbounded_part_header_is_abandoned_instead_of_rescanned() {
        let mut tracker = incremental_tracker();
        let mut body = b"--BOUND\r\n".to_vec();
        // A header block that never terminates: no CRLFCRLF, ever.
        body.extend_from_slice(&vec![b'A'; MAX_PART_HEADER_BYTES + 1]);
        tracker.advance(&body);
        assert!(tracker.files(&body).is_empty());
        assert!(
            tracker.inflight_unparsable,
            "an over-long header block must be abandoned, not retried per frame"
        );
    }

    /// A trigger field that is still STREAMING must not authorize a write.
    ///
    /// Its value is a prefix of the real progress key, so a write under it would create a
    /// session entry at a key `complete()` never removes — with `upload_progress.cleanup`
    /// on, a stale `done => false` record outliving the request. `ready_to_write()` is what
    /// holds the write back until the part closes and the key becomes final.
    #[test]
    fn a_provisional_trigger_key_does_not_authorize_a_write() {
        let mut tracker = incremental_tracker();
        tracker.sid = Some("sid123".to_string());
        let streaming: &[u8] = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nmyk";
        tracker.advance(streaming);
        assert_eq!(tracker.key.as_deref(), Some(&b"myk"[..]), "prefix captured");
        assert!(tracker.key_provisional);
        assert!(
            !tracker.ready_to_write(),
            "a provisional key must not authorize a write"
        );

        let mut closed = streaming.to_vec();
        closed.extend_from_slice(b"ey\r\n--BOUND\r\n");
        tracker.advance(&closed);
        assert_eq!(tracker.key.as_deref(), Some(&b"mykey"[..]), "full key replaces the prefix");
        assert!(!tracker.key_provisional);
        assert!(tracker.ready_to_write());
    }

    /// Issue #885: a boundary longer than RFC 2046's 70-character cap is refused, so no
    /// delimiter search ever pays an attacker-chosen length.
    #[test]
    fn an_over_long_boundary_is_refused() {
        let short = "x".repeat(MAX_BOUNDARY_BYTES);
        let long = "x".repeat(MAX_BOUNDARY_BYTES + 1);
        assert_eq!(
            extract_boundary(&format!("multipart/form-data; boundary={short}")),
            Some(short)
        );
        assert_eq!(
            extract_boundary(&format!("multipart/form-data; boundary={long}")),
            None
        );
    }
}
