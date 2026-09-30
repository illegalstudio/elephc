//! Purpose:
//! Opens local, temporary, process-pipe, memory, and Phar-backed eval streams.
//!
//! Called from:
//! - Filesystem and process stream builtins through `EvalStreamResources`.
//!
//! Key details:
//! - PHP mode parsing and write-back targets are delegated to shared storage types.

use super::*;

/// PHP's wording when nothing described the failure, which is also what a non-filesystem
/// wrapper failure reports: those never reach a libc `open` and so leave no errno behind.
pub(crate) const EVAL_OPEN_DEFAULT_REASON: &str = "No such file or directory";

/// Renders an open failure the way PHP quotes it.
///
/// `to_string()` on a raw-OS error is the platform's `strerror` text plus an " (os error N)"
/// suffix that PHP does not print, so the suffix is trimmed off. Everything else — a path that
/// no wrapper claims, a mode that will not parse — has no errno behind it and falls back to the
/// wording PHP uses when nothing described the failure.
pub(crate) fn eval_open_failure_reason(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error ") {
        Some(cut) => text[..cut].to_string(),
        None => text,
    }
}

impl EvalStreamResources {
    /// Forgets the previous open's reason so a later warning cannot quote a stale one.
    fn clear_last_open_error(&mut self) {
        self.last_open_error = None;
    }

    /// Records why a local open failed, in the platform's own words.
    fn record_open_error(&mut self, error: &std::io::Error) {
        self.last_open_error = Some(eval_open_failure_reason(error));
    }

    /// Returns PHP's reason for the most recent failed open.
    pub(crate) fn last_open_reason(&self) -> String {
        self.last_open_error
            .clone()
            .unwrap_or_else(|| EVAL_OPEN_DEFAULT_REASON.to_string())
    }

    /// Opens a local path using PHP's common `fopen()` mode strings.
    pub(crate) fn open_path(&mut self, path: &str, mode: &str) -> Option<i64> {
        self.clear_last_open_error();
        let mode = EvalOpenMode::parse(mode)?;
        if stream_wrappers::is_php_memory_stream(path) {
            return self.open_ephemeral_stream(path, &mode, &[], None, false);
        }
        if stream_wrappers::is_data_stream(path) {
            let bytes = stream_wrappers::decode_data_uri(path)?;
            return self.open_ephemeral_stream(path, &mode, &bytes, None, false);
        }
        if stream_wrappers::is_phar_stream(path) {
            return self.open_phar_stream(path, &mode);
        }
        if stream_wrappers::is_http_stream(path) && mode.read && !mode.write {
            let bytes = stream_wrappers::read_http_url(path)?;
            return self.open_ephemeral_stream(path, &mode, &bytes, None, false);
        }
        let path = stream_wrappers::local_filesystem_path(path)?;
        let file = match mode.open(&path) {
            Ok(file) => file,
            Err(error) => {
                self.record_open_error(&error);
                return None;
            }
        };
        Some(self.insert(EvalFileStream::new(file, path, mode.label)))
    }

    /// Opens an anonymous temporary file and returns its resource id.
    pub(crate) fn open_tmpfile(&mut self) -> Option<i64> {
        let path = eval_tmpfile_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .ok()?;
        let _ = std::fs::remove_file(&path);
        Some(self.insert(EvalFileStream::new(
            file,
            path.to_string_lossy().into_owned(),
            "w+".to_string(),
        )))
    }

    /// Opens a shell process pipe and returns its stream resource id.
    pub(crate) fn open_process_pipe(&mut self, command: &str, mode: &str) -> Option<i64> {
        let read_mode = match mode.chars().next()? {
            'r' => true,
            'w' => false,
            _ => return None,
        };
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .stdin(if read_mode {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(if read_mode {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .spawn()
            .ok()?;
        let file = if read_mode {
            let stdout = child.stdout.take()?;
            unsafe {
                // The ChildStdout pipe is converted into the File that backs
                // this eval stream; no second owner keeps the fd alive.
                File::from_raw_fd(stdout.into_raw_fd())
            }
        } else {
            let stdin = child.stdin.take()?;
            unsafe {
                // The ChildStdin pipe is converted into the File that backs
                // this eval stream; dropping it before wait sends EOF.
                File::from_raw_fd(stdin.into_raw_fd())
            }
        };
        let id = self.insert(EvalFileStream::new(
            file,
            command.to_string(),
            if read_mode { "r" } else { "w" }.to_string(),
        ));
        self.process_children.insert(id, child);
        Some(id)
    }

}
