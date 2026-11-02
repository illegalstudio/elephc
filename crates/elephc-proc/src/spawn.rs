//! Purpose:
//! Starts one child process for `proc_open()` with PHP's descriptor, cwd, and environment rules.
//!
//! Called from:
//! - `crate::abi` (AOT prelude bridge calls) and the Magician `proc_open()` adapter.
//!
//! Key details:
//! - Everything allocated or fallible happens in the parent before `fork()`; the child runs only
//!   async-signal-safe calls (`fcntl`, `dup2`, `close`, `chdir`, `execv`/`execve`, `write`,
//!   `_exit`), so spawning from a multi-threaded host process is sound.
//! - Every descriptor created here is close-on-exec. A child end only becomes inheritable when
//!   `dup2()` places it on its target number, so no pipe end leaks into the program.
//! - Exec failures travel back through a close-on-exec error pipe: it stays silent on a
//!   successful `execve()` and carries the child's errno on failure, so `spawn()` reports a
//!   missing program without ever returning a handle for a process that never ran.
//! - Sources are first duplicated above every target number, then `dup2()`ed into place, so one
//!   descriptor's target can never clobber another descriptor's source.

use libc::{c_char, c_int, pid_t};
use std::ffi::{CStr, CString};

/// Where one child descriptor comes from.
pub enum Source {
    /// A fresh pipe; `child_reads` selects whether the child end is the read end.
    Pipe {
        /// True when the child reads from the pipe (the parent keeps the write end).
        child_reads: bool,
    },
    /// A file opened by the parent with an `fopen()`-style mode string.
    File {
        /// Path handed to `open(2)`.
        path: CString,
        /// `fopen()` mode such as `r`, `w+`, or `a`.
        mode: Vec<u8>,
    },
    /// An existing parent descriptor, duplicated onto the target.
    Fd(c_int),
    /// `/dev/null`.
    Null,
    /// Another child descriptor, duplicated after the others are in place.
    Redirect(c_int),
}

/// Everything `proc_open()` hands to the spawner.
pub struct Request {
    /// True when `args[0]` is a shell command line for `/bin/sh -c`.
    pub shell: bool,
    /// Shell command line (one element) or the program followed by its arguments.
    pub args: Vec<CString>,
    /// Working directory of the child, or inheritance when absent.
    pub cwd: Option<CString>,
    /// Complete replacement environment as `NAME=value` entries, or inheritance when absent.
    pub env: Option<Vec<CString>>,
    /// Child descriptor numbers with their sources, in `$descriptor_spec` order.
    pub descriptors: Vec<(c_int, Source)>,
}

/// A started child and the parent ends of its pipes.
pub struct Spawned {
    /// Process id of the child.
    pub pid: pid_t,
    /// `(child descriptor number, parent descriptor)` for every pipe, in request order.
    pub pipes: Vec<(c_int, c_int)>,
}

/// Closes descriptors it still owns when dropped, so every early return cleans up.
#[derive(Default)]
struct FdGuard {
    owned: Vec<c_int>,
}

impl FdGuard {
    /// Takes ownership of a descriptor.
    fn add(&mut self, fd: c_int) {
        self.owned.push(fd);
    }

    /// Releases a descriptor from ownership without closing it.
    fn release(&mut self, fd: c_int) {
        self.owned.retain(|owned| *owned != fd);
    }

    /// Closes one owned descriptor now and forgets it.
    fn close(&mut self, fd: c_int) {
        self.release(fd);
        unsafe {
            libc::close(fd);
        }
    }
}

impl Drop for FdGuard {
    /// Closes every descriptor still owned.
    fn drop(&mut self) {
        for fd in self.owned.drain(..) {
            unsafe {
                libc::close(fd);
            }
        }
    }
}

/// Reads the calling thread's errno.
fn errno() -> c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
}

/// Marks a descriptor close-on-exec.
fn set_cloexec(fd: c_int) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
}

/// Creates a pipe whose two ends are both close-on-exec.
fn make_pipe() -> Result<(c_int, c_int), c_int> {
    let mut ends = [0 as c_int; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(errno());
    }
    set_cloexec(ends[0]);
    set_cloexec(ends[1]);
    Ok((ends[0], ends[1]))
}

/// Translates an `fopen()` mode into `open(2)` flags.
pub fn open_flags(mode: &[u8]) -> Option<c_int> {
    let plus = mode.contains(&b'+');
    let access = if plus { libc::O_RDWR } else { libc::O_WRONLY };
    match mode.first()? {
        b'r' => Some(if plus { libc::O_RDWR } else { libc::O_RDONLY }),
        b'w' => Some(access | libc::O_CREAT | libc::O_TRUNC),
        b'a' => Some(access | libc::O_CREAT | libc::O_APPEND),
        b'x' => Some(access | libc::O_CREAT | libc::O_EXCL),
        b'c' => Some(access | libc::O_CREAT),
        _ => None,
    }
}

/// Opens a path close-on-exec and returns the descriptor.
fn open_cloexec(path: &CStr, flags: c_int) -> Result<c_int, c_int> {
    let fd = unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC, 0o666 as libc::c_uint) };
    if fd < 0 {
        return Err(errno());
    }
    set_cloexec(fd);
    Ok(fd)
}

/// Finds the program a bare name refers to by searching the parent's `PATH`.
///
/// A name containing a slash is used as written, so the child's `execve()` reports its failure.
fn resolve_program(name: &CStr) -> Result<CString, c_int> {
    let bytes = name.to_bytes();
    if bytes.is_empty() {
        return Err(libc::ENOENT);
    }
    if bytes.contains(&b'/') {
        return Ok(name.to_owned());
    }
    let path = std::env::var_os("PATH")
        .map(|value| {
            use std::os::unix::ffi::OsStrExt;
            value.as_bytes().to_vec()
        })
        .unwrap_or_else(|| b"/usr/local/bin:/usr/bin:/bin".to_vec());
    let mut denied = false;
    for directory in path.split(|byte| *byte == b':') {
        let mut candidate = if directory.is_empty() { b".".to_vec() } else { directory.to_vec() };
        candidate.push(b'/');
        candidate.extend_from_slice(bytes);
        let Ok(candidate) = CString::new(candidate) else { continue };
        let mut info = unsafe { std::mem::zeroed::<libc::stat>() };
        if unsafe { libc::stat(candidate.as_ptr(), &mut info) } != 0
            || (info.st_mode & libc::S_IFMT) != libc::S_IFREG
        {
            continue;
        }
        if unsafe { libc::access(candidate.as_ptr(), libc::X_OK) } == 0 {
            return Ok(candidate);
        }
        denied = true;
    }
    Err(if denied { libc::EACCES } else { libc::ENOENT })
}

/// Builds a null-terminated pointer array over borrowed C strings.
fn pointer_array(strings: &[CString]) -> Vec<*const c_char> {
    let mut pointers: Vec<*const c_char> = strings.iter().map(|value| value.as_ptr()).collect();
    pointers.push(std::ptr::null());
    pointers
}

/// Reports a child-side failure through the error pipe and exits without running destructors.
///
/// # Safety
/// Must only be called in the forked child; it performs one `write(2)` and `_exit(2)`.
unsafe fn die(error_fd: c_int, code: c_int) -> ! {
    let bytes = code.to_ne_bytes();
    libc::write(error_fd, bytes.as_ptr() as *const libc::c_void, bytes.len());
    libc::_exit(127)
}

/// Starts the child described by `request`.
///
/// Returns the child's pid and the parent ends of its pipes, or the errno that prevented the
/// child from running (a failed `open()`, `fork()`, `chdir()`, or `execve()`).
pub fn spawn(request: &Request) -> Result<Spawned, c_int> {
    if request.args.is_empty() {
        return Err(libc::EINVAL);
    }
    let mut guard = FdGuard::default();
    let mut child_owned: Vec<c_int> = Vec::new();
    let mut mapping: Vec<(c_int, c_int)> = Vec::new();
    let mut redirects: Vec<(c_int, c_int)> = Vec::new();
    let mut parent_pipes: Vec<(c_int, c_int)> = Vec::new();
    for (target, source) in &request.descriptors {
        if *target < 0 {
            return Err(libc::EBADF);
        }
        match source {
            Source::Pipe { child_reads } => {
                let (read_end, write_end) = make_pipe()?;
                guard.add(read_end);
                guard.add(write_end);
                let (child_end, parent_end) =
                    if *child_reads { (read_end, write_end) } else { (write_end, read_end) };
                child_owned.push(child_end);
                mapping.push((*target, child_end));
                parent_pipes.push((*target, parent_end));
            }
            Source::File { path, mode } => {
                let flags = open_flags(mode).ok_or(libc::EINVAL)?;
                let fd = open_cloexec(path, flags)?;
                guard.add(fd);
                child_owned.push(fd);
                mapping.push((*target, fd));
            }
            Source::Fd(source_fd) => mapping.push((*target, *source_fd)),
            Source::Null => {
                let null = CString::new("/dev/null").map_err(|_| libc::EINVAL)?;
                let fd = open_cloexec(&null, libc::O_RDWR)?;
                guard.add(fd);
                child_owned.push(fd);
                mapping.push((*target, fd));
            }
            Source::Redirect(from) => {
                let defined = request.descriptors.iter().any(|(number, other)| {
                    number == from && !matches!(other, Source::Redirect(_))
                });
                if !defined && *from > 2 {
                    return Err(libc::EBADF);
                }
                redirects.push((*target, *from));
            }
        }
    }

    let shell = CString::new("/bin/sh").map_err(|_| libc::EINVAL)?;
    let (program, argv) = if request.shell {
        let sh = CString::new("sh").map_err(|_| libc::EINVAL)?;
        let flag = CString::new("-c").map_err(|_| libc::EINVAL)?;
        (shell, vec![sh, flag, request.args[0].clone()])
    } else {
        (resolve_program(&request.args[0])?, request.args.clone())
    };
    let argv_pointers = pointer_array(&argv);
    let env_pointers = request.env.as_ref().map(|env| pointer_array(env));

    // Every duplicated source and the error pipe live above every number the child assigns.
    let highest_target = request
        .descriptors
        .iter()
        .map(|(target, _)| *target)
        .max()
        .unwrap_or(2)
        .max(2);
    let high = highest_target + 1;
    let (error_read, error_write) = make_pipe()?;
    guard.add(error_read);
    guard.add(error_write);
    let error_write_high = unsafe { libc::fcntl(error_write, libc::F_DUPFD_CLOEXEC, high) };
    if error_write_high < 0 {
        return Err(errno());
    }
    guard.add(error_write_high);
    guard.close(error_write);

    // Sized before fork so the child never allocates while staging source duplicates.
    let mut slots: Vec<(c_int, c_int)> = vec![(0, 0); mapping.len()];
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(errno());
    }
    if pid == 0 {
        // Child: only async-signal-safe calls from here to `execve()` or `_exit()`.
        unsafe {
            for (index, (target, source)) in mapping.iter().enumerate() {
                let duplicate = libc::fcntl(*source, libc::F_DUPFD, high);
                if duplicate < 0 {
                    die(error_write_high, errno());
                }
                slots[index] = (*target, duplicate);
            }
            for slot in slots.iter() {
                if libc::dup2(slot.1, slot.0) < 0 {
                    die(error_write_high, errno());
                }
            }
            for slot in slots.iter() {
                libc::close(slot.1);
            }
            for (target, from) in &redirects {
                if libc::dup2(*from, *target) < 0 {
                    die(error_write_high, errno());
                }
            }
            if let Some(cwd) = &request.cwd {
                if libc::chdir(cwd.as_ptr()) != 0 {
                    die(error_write_high, errno());
                }
            }
            // php's CLI ignores SIGPIPE and children inherit the disposition, so a child writing
            // to a pipe the parent closed sees EPIPE and exits with a status, not a signal.
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            match &env_pointers {
                Some(env) => {
                    libc::execve(program.as_ptr(), argv_pointers.as_ptr(), env.as_ptr());
                }
                None => {
                    libc::execv(program.as_ptr(), argv_pointers.as_ptr());
                }
            }
            die(error_write_high, errno());
        }
    }

    // Parent: close the child's ends, then wait for the exec verdict on the error pipe.
    for fd in child_owned {
        guard.close(fd);
    }
    guard.close(error_write_high);
    let mut code = [0u8; 4];
    let read = loop {
        let result =
            unsafe { libc::read(error_read, code.as_mut_ptr() as *mut libc::c_void, code.len()) };
        if result < 0 && errno() == libc::EINTR {
            continue;
        }
        break result;
    };
    if read == code.len() as isize {
        let failure = c_int::from_ne_bytes(code);
        let mut status = 0 as c_int;
        loop {
            let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
            if reaped < 0 && errno() == libc::EINTR {
                continue;
            }
            break;
        }
        return Err(failure);
    }
    for (_, parent_end) in &parent_pipes {
        guard.release(*parent_end);
    }
    Ok(Spawned { pid, pipes: parent_pipes })
}
