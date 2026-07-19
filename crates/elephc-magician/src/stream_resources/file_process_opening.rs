//! Purpose:
//! Opens local, temporary, process-pipe, memory, and Phar-backed eval streams.
//!
//! Called from:
//! - Filesystem and process stream builtins through `EvalStreamResources`.
//!
//! Key details:
//! - PHP mode parsing and write-back targets are delegated to shared storage types.

use super::*;

impl EvalStreamResources {
    /// Opens a local path using PHP's common `fopen()` mode strings.
    pub(crate) fn open_path(&mut self, path: &str, mode: &str) -> Option<i64> {
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
        let file = mode.open(&path).ok()?;
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
