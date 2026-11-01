//! Purpose:
//! Owns the eval-side `proc_open()` process handles and the pipe streams each one opened.
//!
//! Called from:
//! - The `proc_open` / `proc_close` / `proc_get_status` / `proc_terminate` eval builtins.
//!
//! Key details:
//! - A process handle is a resource of type `process` keyed in the shared `elephc_proc` status
//!   table by its own payload, so the bridge crate answers status and close queries exactly as it
//!   does for compiled programs.
//! - The pipe streams are ordinary eval file streams. `close_process` closes any the caller left
//!   open before it waits, as php's process-resource destructor does.

use super::*;

impl EvalStreamResources {
    /// Wraps a parent pipe end as an eval stream and returns its resource id.
    pub(crate) fn open_pipe_stream(&mut self, fd: i32, label: &str, readable: bool) -> i64 {
        // SAFETY: the bridge hands over sole ownership of a freshly created pipe end, and
        // nothing else wraps this descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        self.insert(EvalFileStream::new(
            file,
            label.to_string(),
            if readable { "r" } else { "w" }.to_string(),
        ))
    }

    /// Returns the native descriptor behind a stream payload.
    ///
    /// An eval-owned stream is looked up in the table; a payload below the eval base is a
    /// descriptor the compiled program opened, which is its own number.
    pub(crate) fn raw_descriptor(&self, id: i64) -> Option<i32> {
        if id >= EVAL_RESOURCE_PAYLOAD_BASE {
            return self.streams.get(&id).map(|stream| stream.file.as_raw_fd());
        }
        i32::try_from(id).ok()
    }

    /// Registers a started process and returns its handle's resource id.
    pub(crate) fn open_process_handle(
        &mut self,
        pid: libc::pid_t,
        command: &str,
        pipes: Vec<i64>,
    ) -> i64 {
        let id = self.take_next_id();
        self.resource_types.insert(id, "process");
        self.process_handles.insert(id, pipes);
        elephc_proc::table::register(id, pid, command, &[]);
        id
    }

    /// Returns whether `id` is a live process handle.
    pub(crate) fn is_process_handle(&self, id: i64) -> bool {
        self.process_handles.contains_key(&id)
    }

    /// Closes any pipes still open, waits for the child, and returns `proc_close()`'s result.
    pub(crate) fn close_process(&mut self, id: i64) -> Option<i64> {
        let pipes = self.process_handles.remove(&id)?;
        for pipe in pipes {
            self.close(pipe);
        }
        elephc_proc::table::close(id)
    }
}
