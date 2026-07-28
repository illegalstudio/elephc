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
        #[cfg(unix)]
        let mut shell = {
            let mut shell = Command::new("/bin/sh");
            shell.arg("-c");
            shell
        };
        #[cfg(windows)]
        let mut shell = {
            let mut shell = Command::new("cmd.exe");
            shell.arg("/C");
            shell
        };
        let mut child = shell
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
        let stream = if read_mode {
            EvalFileStream::new_child_stdout(
                child.stdout.take()?,
                command.to_string(),
                "r".to_string(),
            )
        } else {
            EvalFileStream::new_child_stdin(
                child.stdin.take()?,
                command.to_string(),
                "w".to_string(),
            )
        };
        let id = self.insert(stream);
        self.process_children.insert(id, child);
        Some(id)
    }

    /// Starts a command with materialized child descriptors and returns both the
    /// process resource and every parent-side pipe resource.
    pub(crate) fn open_process(
        &mut self,
        command: &str,
        descriptors: &[Option<EvalProcDescriptor>; 3],
        cwd: Option<&str>,
        env: Option<&[(String, String)]>,
        bypass_shell: bool,
    ) -> Option<EvalProcOpenResult> {
        let mut child_handles: [Option<File>; 3] = std::array::from_fn(|_| None);
        let mut parent_pipes = Vec::new();
        for (descriptor, spec) in descriptors.iter().enumerate() {
            let Some(spec) = spec else {
                continue;
            };
            match spec {
                EvalProcDescriptor::Pipe { child_reads } => {
                    let (read, write) = eval_anonymous_pipe().ok()?;
                    let (child, parent, mode) = if *child_reads {
                        (read, write, "w")
                    } else {
                        (write, read, "r")
                    };
                    child_handles[descriptor] = Some(child);
                    parent_pipes.push((descriptor as i64, parent, mode));
                }
                EvalProcDescriptor::File { path, mode } => {
                    let parsed = EvalOpenMode::parse(mode)?;
                    child_handles[descriptor] = Some(parsed.open(path).ok()?);
                }
                EvalProcDescriptor::Redirect(_) => {}
            }
        }
        for _ in 0..descriptors.len() {
            let mut changed = false;
            for (descriptor, spec) in descriptors.iter().enumerate() {
                let Some(EvalProcDescriptor::Redirect(target)) = spec else {
                    continue;
                };
                if child_handles[descriptor].is_none() {
                    if let Some(target_handle) = child_handles.get(*target)?.as_ref() {
                        child_handles[descriptor] = Some(target_handle.try_clone().ok()?);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        if descriptors
            .iter()
            .enumerate()
            .any(|(index, spec)| spec.is_some() && child_handles[index].is_none())
        {
            return None;
        }

        let mut command_builder;
        if bypass_shell {
            let mut parts = command.split_whitespace();
            command_builder = Command::new(parts.next()?);
            command_builder.args(parts);
        } else {
        #[cfg(unix)]
        {
            let mut shell = Command::new("/bin/sh");
            shell.arg("-c");
            command_builder = shell;
        }
        #[cfg(windows)]
        {
            let mut shell = Command::new("cmd.exe");
            shell.arg("/C");
            command_builder = shell;
        }
        command_builder.arg(command);
        }
        command_builder
            .stdin(child_handles[0].take().map_or_else(Stdio::null, Stdio::from))
            .stdout(child_handles[1].take().map_or_else(Stdio::null, Stdio::from))
            .stderr(child_handles[2].take().map_or_else(Stdio::null, Stdio::from));
        if let Some(cwd) = cwd {
            command_builder.current_dir(cwd);
        }
        if let Some(env) = env {
            command_builder.env_clear().envs(env.iter().cloned());
        }
        let child = command_builder.spawn().ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.process_children.insert(id, child);
        self.process_commands.insert(id, command.to_string());
        let mut pipes = Vec::with_capacity(parent_pipes.len());
        for (descriptor, parent, mode) in parent_pipes {
            let pipe_id = self.insert(EvalFileStream::new(
                parent,
                format!("proc://{command}/{descriptor}"),
                mode.to_string(),
            ));
            pipes.push((descriptor, pipe_id));
        }
        Some(EvalProcOpenResult {
            process_id: id,
            pipes,
        })
    }

}

/// Creates a parent/child anonymous byte pipe as owned file handles.
#[cfg(unix)]
fn eval_anonymous_pipe() -> io::Result<(File, File)> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) })
}

/// Creates a Windows anonymous pipe; Rust's process launcher duplicates the
/// selected child end with the inheritance required for stdio.
#[cfg(windows)]
fn eval_anonymous_pipe() -> io::Result<(File, File)> {
    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        /// Creates the anonymous kernel pipe used by the child process.
        fn CreatePipe(
            read: *mut *mut c_void,
            write: *mut *mut c_void,
            attributes: *mut SecurityAttributes,
            size: u32,
        ) -> i32;
    }
    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    let mut attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: std::ptr::null_mut(),
        inherit: 0,
    };
    if unsafe { CreatePipe(&mut read, &mut write, &mut attributes, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (File::from_raw_handle(read), File::from_raw_handle(write)) })
}
