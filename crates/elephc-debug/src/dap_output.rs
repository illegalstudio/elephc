//! Purpose:
//! Captures the debuggee's stdout/stderr while a DAP session is active and
//! forwards each chunk as a DAP `output` event.
//!
//! Called from:
//! - `Debugger::attach_dap()` when a launch or attach DAP channel is established.
//!
//! Key details:
//! - The pipe write ends replace fd 1/2; the saved descriptors are restored when
//!   the session ends so a detached program keeps its original terminal output.
//! - Each stream has its own pipe and thread, preserving DAP `stdout`/`stderr`
//!   categories. The DAP writer itself is shared and sequence-numbered by
//!   `DapConnection`.
//! - Output chunks are lossy UTF-8 because DAP carries JSON text.

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::thread::JoinHandle;

use super::dap::DapOutputHandle;

/// Keeps the debuggee's stdout/stderr redirected to DAP output for one session.
pub(crate) struct DapOutputPump {
    original_stdout: Option<OwnedFd>,
    original_stderr: Option<OwnedFd>,
    threads: Vec<JoinHandle<()>>,
}

impl DapOutputPump {
    /// Redirects fd 1/2 to pipes and starts one forwarding thread per stream.
    pub(crate) fn install(handle: DapOutputHandle) -> io::Result<Self> {
        let original_stdout = duplicate(1)?;
        let original_stderr = duplicate(2)?;
        let (stdout_read, stdout_write) = pipe()?;
        let (stderr_read, stderr_write) = pipe()?;
        if unsafe { libc::dup2(stdout_write.as_raw_fd(), 1) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(stderr_write.as_raw_fd(), 2) } < 0 {
            unsafe {
                libc::dup2(original_stdout.as_raw_fd(), 1);
            }
            return Err(io::Error::last_os_error());
        }
        drop(stdout_write);
        drop(stderr_write);
        let threads = vec![
            spawn_pump(stdout_read.into_raw_fd(), "stdout", handle.clone()),
            spawn_pump(stderr_read.into_raw_fd(), "stderr", handle),
        ];
        Ok(Self {
            original_stdout: Some(original_stdout),
            original_stderr: Some(original_stderr),
            threads,
        })
    }

    /// Restores the saved stdout/stderr and waits for the pump threads to drain.
    fn restore(&mut self) {
        if let Some(fd) = self.original_stdout.take() {
            unsafe {
                libc::dup2(fd.as_raw_fd(), 1);
            }
        }
        if let Some(fd) = self.original_stderr.take() {
            unsafe {
                libc::dup2(fd.as_raw_fd(), 2);
            }
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for DapOutputPump {
    /// Restores the descriptors even when the session is torn down by unwinding.
    fn drop(&mut self) {
        self.restore();
    }
}

/// Duplicates one descriptor for later restoration.
fn duplicate(fd: RawFd) -> io::Result<OwnedFd> {
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
}

/// Creates one pipe with owned read/write ends.
fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe {
        (
            OwnedFd::from_raw_fd(fds[0]),
            OwnedFd::from_raw_fd(fds[1]),
        )
    })
}

/// Reads one pipe until EOF and emits each chunk as a DAP output event.
fn spawn_pump(fd: RawFd, category: &'static str, handle: DapOutputHandle) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut file = unsafe { File::from_raw_fd(fd) };
        let mut buffer = [0u8; 4096];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                    if handle.output(category, &text).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    })
}
