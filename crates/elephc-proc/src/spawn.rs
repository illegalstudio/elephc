//! Purpose:
//! Stages one `proc_open()` call — command, cwd, environment and the descriptor table —
//! and spawns it with `posix_spawn`/`posix_spawnp`, as PHP 8.5 itself does on macOS and
//! Linux.
//!
//! Called from:
//! - The `elephc_proc_builder_*` / `elephc_proc_spawn` C ABI in `crate`.
//!
//! Key details:
//! - Descriptors are created EAGERLY, in spec order, exactly like php-src's walk: a pipe
//!   or file that cannot be opened fails that entry before later entries are looked at.
//!   Dropping a builder closes every descriptor it still owns.
//! - Every descriptor the builder opens is close-on-exec. The child receives its ends
//!   through `posix_spawn_file_actions_adddup2`, which clears the flag on the target, so
//!   no other parent descriptor (including another process's pipe) leaks into the child.
//! - Child ends are first moved above the highest requested child descriptor number, so
//!   one `dup2` can never overwrite the source of a later one.
//! - Failure texts are php-src's warning texts, complete with their `proc_open(...)`
//!   prefix, so the prelude hands them to the engine warning unchanged.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

extern "C" {
    /// glibc 2.29+, musl 1.1.24+ and Darwin 10.15+ all export this; libc only binds it on
    /// some targets, so the bridge declares it once for every target.
    fn posix_spawn_file_actions_addchdir_np(
        actions: *mut libc::posix_spawn_file_actions_t,
        path: *const c_char,
    ) -> c_int;
}

/// One child descriptor staged by the builder.
pub(crate) struct Descriptor {
    /// The descriptor number the child sees (the spec array key).
    pub(crate) index: i64,
    /// The parent-side descriptor duplicated into the child, owned and close-on-exec.
    pub(crate) child_end: c_int,
    /// The parent's end of a pipe, owned and close-on-exec, handed to PHP after spawning.
    pub(crate) parent_end: Option<c_int>,
}

/// Everything one `proc_open()` call stages before spawning.
#[derive(Default)]
pub(crate) struct Builder {
    /// `/bin/sh -c` command text for the string form.
    pub(crate) shell: Option<CString>,
    /// argv for the array form; `argv[0]` is looked up on the parent's `PATH`.
    pub(crate) args: Vec<CString>,
    /// Working directory applied in the child before exec.
    pub(crate) cwd: Option<CString>,
    /// Replacement environment, or `None` to inherit the parent's.
    pub(crate) env: Option<Vec<CString>>,
    /// Descriptors in spec order.
    pub(crate) descriptors: Vec<Descriptor>,
}

impl Drop for Builder {
    /// Closes every descriptor the builder still owns.
    fn drop(&mut self) {
        for descriptor in self.descriptors.drain(..) {
            close_quietly(descriptor.child_end);
            if let Some(parent) = descriptor.parent_end {
                close_quietly(parent);
            }
        }
    }
}

/// A successfully spawned child, before it is registered in the process table.
pub(crate) struct Spawned {
    /// The child's process id.
    pub(crate) pid: libc::pid_t,
    /// `command` as `proc_get_status()` reports it: the shell text, or `argv[0]`.
    pub(crate) command: CString,
    /// `(child descriptor number, parent pipe end)` for every pipe, in spec order.
    pub(crate) pipes: Vec<(i64, c_int)>,
}

/// Closes a descriptor and ignores the result (used on cleanup paths only).
pub(crate) fn close_quietly(fd: c_int) {
    if fd >= 0 {
        unsafe {
            libc::close(fd);
        }
    }
}

/// Returns the C library's message for `errno`.
pub(crate) fn strerror(errno: c_int) -> String {
    unsafe { CStr::from_ptr(libc::strerror(errno)) }
        .to_string_lossy()
        .into_owned()
}

/// Returns the calling thread's current errno.
pub(crate) fn last_errno() -> c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
}

/// Marks `fd` close-on-exec (Linux creates its pipes with `pipe2(O_CLOEXEC)` instead).
#[cfg(not(target_os = "linux"))]
fn set_cloexec(fd: c_int) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
}

/// Creates a pipe whose two ends are both close-on-exec.
fn cloexec_pipe() -> Result<[c_int; 2], c_int> {
    let mut fds = [-1 as c_int; 2];
    #[cfg(target_os = "linux")]
    let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(target_os = "linux"))]
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if result != 0 {
        return Err(last_errno());
    }
    #[cfg(not(target_os = "linux"))]
    for fd in fds {
        set_cloexec(fd);
    }
    Ok(fds)
}

/// Duplicates `fd` into a fresh close-on-exec descriptor numbered at least `floor`.
fn dup_cloexec(fd: c_int, floor: c_int) -> Result<c_int, c_int> {
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, floor) };
    if copy < 0 {
        Err(last_errno())
    } else {
        Ok(copy)
    }
}

/// Translates a PHP `fopen()` mode into `open(2)` flags, or `None` for an invalid mode.
fn open_flags(mode: &str) -> Option<c_int> {
    let mut chars = mode.chars();
    let base = chars.next()?;
    let plus = chars.as_str().contains('+');
    let access = if plus { libc::O_RDWR } else if base == 'r' { libc::O_RDONLY } else { libc::O_WRONLY };
    let extra = match base {
        'r' => 0,
        'w' => libc::O_CREAT | libc::O_TRUNC,
        'a' => libc::O_CREAT | libc::O_APPEND,
        'x' => libc::O_CREAT | libc::O_EXCL,
        'c' => libc::O_CREAT,
        _ => return None,
    };
    Some(access | extra | libc::O_CLOEXEC)
}

impl Builder {
    /// Records one staged descriptor.
    fn push(&mut self, index: i64, child_end: c_int, parent_end: Option<c_int>) {
        self.descriptors.push(Descriptor { index, child_end, parent_end });
    }

    /// Stages `['pipe', $mode]`: a mode starting with `w` lets the child write and the
    /// parent read; anything else is the reverse (php-src tests only the first byte).
    pub(crate) fn pipe(&mut self, index: i64, child_writes: bool) -> Result<(), String> {
        let [read, write] = cloexec_pipe()
            .map_err(|errno| format!("proc_open(): Unable to create pipe {}", strerror(errno)))?;
        if child_writes {
            self.push(index, write, Some(read));
        } else {
            self.push(index, read, Some(write));
        }
        Ok(())
    }

    /// Stages `['file', $path, $mode]` by opening the file with fopen-mode semantics.
    pub(crate) fn file(&mut self, index: i64, path: &CStr, mode: &str) -> Result<(), String> {
        let display = path.to_string_lossy();
        let Some(flags) = open_flags(mode) else {
            return Err(format!(
                "proc_open({display}): Failed to open stream: {}",
                strerror(libc::EINVAL)
            ));
        };
        let fd = unsafe { libc::open(path.as_ptr(), flags, 0o666 as libc::c_uint) };
        if fd < 0 {
            return Err(format!(
                "proc_open({display}): Failed to open stream: {}",
                strerror(last_errno())
            ));
        }
        self.push(index, fd, None);
        Ok(())
    }

    /// Stages an existing stream's descriptor: the child gets a duplicate of it.
    pub(crate) fn inherit(&mut self, index: i64, fd: c_int) -> Result<(), String> {
        let copy = dup_cloexec(fd, 0).map_err(|errno| {
            format!(
                "proc_open(): Unable to dup File-Handle for descriptor {index} - {}",
                strerror(errno)
            )
        })?;
        self.push(index, copy, None);
        Ok(())
    }

    /// Stages `['null']`: the child descriptor is `/dev/null` opened read-write.
    pub(crate) fn null(&mut self, index: i64) -> Result<(), String> {
        let fd = unsafe {
            libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC)
        };
        if fd < 0 {
            return Err(format!(
                "proc_open(): Failed to open /dev/null: {}",
                strerror(last_errno())
            ));
        }
        self.push(index, fd, None);
        Ok(())
    }

    /// Stages `['redirect', $target]`: the child descriptor duplicates an earlier spec
    /// entry's child end, or the parent's own 0/1/2 when the spec does not name the target.
    pub(crate) fn redirect(&mut self, index: i64, target: i64) -> Result<(), String> {
        let source = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.index == target)
            .map(|descriptor| descriptor.child_end)
            .or(if (0..=2).contains(&target) { Some(target as c_int) } else { None })
            .ok_or_else(|| format!("proc_open(): Redirection target {target} not found"))?;
        let copy = dup_cloexec(source, 0).map_err(|errno| {
            format!("proc_open(): Failed to dup() for descriptor {index}: {}", strerror(errno))
        })?;
        self.push(index, copy, None);
        Ok(())
    }

    /// Spawns the staged command. Consumes the builder: on success the child ends are
    /// closed and the pipe parent ends move into [`Spawned`]; on failure everything closes.
    pub(crate) fn spawn(mut self) -> Result<Spawned, String> {
        let floor = self
            .descriptors
            .iter()
            .map(|descriptor| descriptor.index)
            .filter(|index| *index >= 0)
            .max()
            .map_or(3, |max| (max + 1).max(3));
        let floor = c_int::try_from(floor).unwrap_or(c_int::MAX);
        for descriptor in &mut self.descriptors {
            if descriptor.child_end < floor {
                if let Ok(moved) = dup_cloexec(descriptor.child_end, floor) {
                    close_quietly(descriptor.child_end);
                    descriptor.child_end = moved;
                }
            }
        }
        let mut actions = std::mem::MaybeUninit::<libc::posix_spawn_file_actions_t>::uninit();
        if unsafe { libc::posix_spawn_file_actions_init(actions.as_mut_ptr()) } != 0 {
            return Err(format!("proc_open(): posix_spawn() failed: {}", strerror(last_errno())));
        }
        let result = self.spawn_with_actions(actions.as_mut_ptr());
        unsafe {
            libc::posix_spawn_file_actions_destroy(actions.as_mut_ptr());
        }
        result
    }

    /// Fills the file actions, spawns, and hands back the parent ends on success.
    fn spawn_with_actions(
        &mut self,
        actions: *mut libc::posix_spawn_file_actions_t,
    ) -> Result<Spawned, String> {
        for descriptor in &self.descriptors {
            let target = c_int::try_from(descriptor.index).unwrap_or(-1);
            let result = unsafe {
                libc::posix_spawn_file_actions_adddup2(actions, descriptor.child_end, target)
            };
            if result != 0 {
                return Err(format!(
                    "proc_open(): Unable to copy file descriptor {} (for pipe) into file descriptor {}: {}",
                    descriptor.child_end,
                    descriptor.index,
                    strerror(result)
                ));
            }
        }
        if let Some(cwd) = &self.cwd {
            let result = unsafe { posix_spawn_file_actions_addchdir_np(actions, cwd.as_ptr()) };
            if result != 0 {
                return Err(format!(
                    "proc_open(): posix_spawn_file_actions_addchdir_np() failed: {}",
                    strerror(result)
                ));
            }
        }
        let shell_argv;
        let (path, argv, command): (*const c_char, Vec<*mut c_char>, CString) = match &self.shell {
            Some(command) => {
                shell_argv = [c"sh".as_ptr(), c"-c".as_ptr(), command.as_ptr()];
                let mut argv: Vec<*mut c_char> =
                    shell_argv.iter().map(|arg| arg.cast_mut()).collect();
                argv.push(std::ptr::null_mut());
                (c"/bin/sh".as_ptr(), argv, command.clone())
            }
            None => {
                let first = self.args.first().ok_or_else(|| {
                    format!("proc_open(): posix_spawn() failed: {}", strerror(libc::EINVAL))
                })?;
                let mut argv: Vec<*mut c_char> =
                    self.args.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
                argv.push(std::ptr::null_mut());
                (first.as_ptr(), argv, first.clone())
            }
        };
        let env_pointers: Option<Vec<*mut c_char>> = self.env.as_ref().map(|entries| {
            let mut pointers: Vec<*mut c_char> =
                entries.iter().map(|entry| entry.as_ptr().cast_mut()).collect();
            pointers.push(std::ptr::null_mut());
            pointers
        });
        let envp = match &env_pointers {
            Some(pointers) => pointers.as_ptr(),
            None => current_environ(),
        };
        let mut pid: libc::pid_t = 0;
        let result = {
            let _signals = elephc_process_context::SpawnPermit::new();
            unsafe {
                if self.shell.is_some() {
                    libc::posix_spawn(&mut pid, path, actions, std::ptr::null(), argv.as_ptr(), envp)
                } else {
                    libc::posix_spawnp(&mut pid, path, actions, std::ptr::null(), argv.as_ptr(), envp)
                }
            }
        };
        if result != 0 {
            return Err(format!("proc_open(): posix_spawn() failed: {}", strerror(result)));
        }
        let mut pipes = Vec::new();
        for descriptor in self.descriptors.drain(..) {
            close_quietly(descriptor.child_end);
            if let Some(parent) = descriptor.parent_end {
                pipes.push((descriptor.index, parent));
            }
        }
        Ok(Spawned { pid, command, pipes })
    }
}

/// Returns the process environment block the child inherits when `$env_vars` is null.
#[cfg(target_vendor = "apple")]
fn current_environ() -> *const *mut c_char {
    unsafe { *libc::_NSGetEnviron() as *const *mut c_char }
}

/// Returns the process environment block the child inherits when `$env_vars` is null.
#[cfg(not(target_vendor = "apple"))]
fn current_environ() -> *const *mut c_char {
    extern "C" {
        static environ: *const *mut c_char;
    }
    unsafe { environ }
}
