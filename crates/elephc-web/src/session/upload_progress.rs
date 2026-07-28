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
//! - Progress writes use independent platform-locked `open -> read -> modify ->
//!   write -> close` cycles on `<save_path>/sess_<id>`; they NEVER touch the
//!   handler's persistent `state::SESSION_FD`, so the short lock is never held
//!   across the whole upload. The handler runs after the drain, so all progress
//!   writes are flushed and unlocked before `session_start` locks the file.
//! - The tracker re-parses the bytes-received-so-far on each freq threshold
//!   (re-parse-on-threshold), boundary-aware for the trailing in-flight part, so
//!   it degrades gracefully on a truncated/adversarial body instead of relying
//!   on a fragile hand-rolled byte-at-a-time state machine. It never panics.
//! - The RMW preserves unrelated entries under all three registered session
//!   serializers and accepts Cookie, GET, or multipart POST session IDs.

use std::ffi::CStr;
use std::io::Read;
use std::time::Instant;

use super::file_io::{
    configured_session_file_path, lock_exclusive, open_session_file, parse_save_path,
    write_file_in_place,
};

/// Per-file progress snapshot mirroring one entry of PHP's `files` sub-array.
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
        let (key, sid, files) = self.snapshot(body);
        if self.key.is_none() {
            self.key = key;
        }
        if self.sid.is_none() {
            self.sid = sid.filter(|value| valid_sid(value));
        }
        if self.sid.is_none() {
            return;
        }
        let Some(full_key) = self.full_key() else {
            return; // No trigger field yet: nothing to write.
        };
        let bytes = body.len();
        let enough_bytes = bytes.saturating_sub(self.last_write_bytes) >= self.freq_bytes;
        let enough_time = self.last_write_time.elapsed().as_secs_f64() >= self.min_freq;
        if !(enough_bytes && enough_time) {
            return;
        }
        let value = self.serialize_progress(&files, bytes, false);
        if self.write_entry(&full_key, &value) {
            self.last_write_bytes = bytes;
            self.last_write_time = Instant::now();
        }
    }

    /// Finalizes progress once the body is fully drained: marks every file and
    /// the whole upload `done`, then does one last write — removing the entry
    /// when `cleanup` is on, or persisting the `done => true` snapshot otherwise.
    pub(crate) fn complete(&mut self, body: &[u8]) {
        let (key, sid, mut files) = self.snapshot(body);
        if self.key.is_none() {
            self.key = key;
        }
        if self.sid.is_none() {
            self.sid = sid.filter(|value| valid_sid(value));
        }
        if self.sid.is_none() {
            return;
        }
        let Some(full_key) = self.full_key() else {
            return; // No trigger field ever seen: nothing to finalize.
        };
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

    /// Re-parses the received bytes into the progress key, optional earlier
    /// multipart session ID, and per-file snapshot. Completed parts (between
    /// two boundaries) are parsed fully; the trailing in-flight part (after the
    /// last boundary, header block complete but no closing boundary yet)
    /// contributes a `done=false` file with its partial `bytes_processed`.
    /// Tolerant of truncation — never panics.
    fn snapshot(&self, body: &[u8]) -> (Option<Vec<u8>>, Option<String>, Vec<FileProgress>) {
        let mut files = Vec::new();
        let mut key: Option<Vec<u8>> = None;
        let mut sid: Option<String> = None;
        let positions = find_all(body, &self.delim);
        // Completed parts sit between consecutive boundary delimiters.
        for pair in positions.windows(2) {
            let seg = &body[pair[0] + self.delim.len()..pair[1]];
            let seg = strip_prefix(seg, b"\r\n");
            let seg = strip_suffix(seg, b"\r\n");
            if let Some((name, filename, content)) = parse_part(seg) {
                if let Some(fname) = filename {
                    files.push(FileProgress {
                        field_name: name,
                        name: fname,
                        bytes_processed: content.len(),
                        done: true,
                    });
                } else if key.is_none() && name == self.name_field {
                    key = Some(content.to_vec());
                } else if sid.is_none() && name == self.sid_field {
                    sid = Some(String::from_utf8_lossy(content).into_owned());
                }
            }
        }
        // Trailing in-flight part after the final boundary, if any.
        if let Some(&last) = positions.last() {
            let tail = &body[last + self.delim.len()..];
            let tail = strip_prefix(tail, b"\r\n");
            // A closing boundary marker ("--") means the upload has ended.
            if !tail.starts_with(b"--") {
                if let Some(hdr_end) = find(tail, b"\r\n\r\n") {
                    if let Some((name, filename, _)) = parse_part(tail) {
                        let content = &tail[hdr_end + 4..];
                        if let Some(fname) = filename {
                            files.push(FileProgress {
                                field_name: name,
                                name: fname,
                                bytes_processed: content.len(),
                                done: false,
                            });
                        } else if key.is_none() && name == self.name_field {
                            // Rare: trigger field still streaming — take what we have.
                            key = Some(content.to_vec());
                        }
                    }
                }
            }
        }
        (key, sid, files)
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

    /// Runs `edit` under an independent platform-locked `open -> read ->
    /// truncate+write -> close` cycle on the session file. Never uses the
    /// handler's persistent file. Closes every transient file and releases the
    /// lock on every path. Returns true when the edit was written.
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
        let mut file = match open_session_file(&path, config.mode) {
            Ok(f) => f,
            Err(_) => return false,
        };
        if !lock_exclusive(&file) {
            return false;
        }
        let mut data = Vec::new();
        if file.read_to_end(&mut data).is_err() {
            return false;
        }
        let mut ok = false;
        if let Some(new_data) = edit(&data) {
            ok = write_file_in_place(&mut file, &new_data).is_ok();
        }
        drop(file); // closes the file and releases its OS lock
        ok
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

    // SAFETY: single-threaded per worker; getters only read/lazy-init statics.
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
            if !v.is_empty() {
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

/// Returns the byte offsets of every non-overlapping occurrence of `needle`.
fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Returns the offset of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
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
        let t = Tracker {
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
        };
        // Completed trigger field, then a file part still streaming (no closing
        // boundary yet).
        let body = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nmykey\r\n--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nPARTIAL";
        let (key, sid, files) = t.snapshot(body);
        assert_eq!(key.as_deref(), Some(&b"mykey"[..]));
        assert_eq!(sid, None);
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
        let tracker = Tracker {
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
        };
        let body = b"--BOUND\r\nContent-Disposition: form-data; name=\"PHPSESSID\"\r\n\r\nsid123\r\n--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nkey\r\n--BOUND--\r\n";
        let (key, sid, files) = tracker.snapshot(body);
        assert_eq!(key.as_deref(), Some(&b"key"[..]));
        assert_eq!(sid.as_deref(), Some("sid123"));
        assert!(files.is_empty());
    }
}
