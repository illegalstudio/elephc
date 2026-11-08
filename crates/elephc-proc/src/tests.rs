//! Purpose:
//! Exercises the proc bridge ABI against real child processes: pipes, files, redirects,
//! cwd/env, spawn failures, cached waits, signals and `proc_close()` statuses.
//!
//! Called from:
//! - `cargo test -p elephc-proc` through Rust's test harness.
//!
//! Key details:
//! - Every case spawns `/bin/sh` or a PATH tool, so it needs a Unix host (macOS or Linux).
//! - Reads use blocking `read(2)` on the returned parent descriptors, which see EOF once
//!   the child exits because every builder descriptor is close-on-exec.

use super::*;
use std::os::raw::c_int;

/// Reads a descriptor to EOF and closes it.
fn read_all(fd: i64) -> String {
    let mut out = Vec::new();
    let mut buffer = [0u8; 256];
    loop {
        let read = unsafe { libc::read(fd as c_int, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read <= 0 {
            break;
        }
        out.extend_from_slice(&buffer[..read as usize]);
    }
    unsafe { libc::close(fd as c_int) };
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns the inode behind a descriptor, or `None` when it is closed.
fn inode(fd: c_int) -> Option<u64> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return None;
    }
    Some(unsafe { stat.assume_init() }.st_ino as u64)
}

/// Returns the last recorded failure text.
fn last_error() -> String {
    unsafe { CStr::from_ptr(elephc_proc_last_error()) }.to_string_lossy().into_owned()
}

/// Spawns a shell command whose stdout is a pipe and returns `(process, stdout fd)`.
fn shell_with_stdout(command: &CStr) -> (i64, i64) {
    let builder = elephc_proc_builder_new();
    assert_eq!(unsafe { elephc_proc_builder_shell(builder, command.as_ptr()) }, 1);
    assert_eq!(unsafe { elephc_proc_builder_pipe(builder, 1, c"w".as_ptr()) }, 1);
    let process = elephc_proc_spawn(builder);
    assert!(process >= PROCESS_ID_BASE, "{}", last_error());
    assert_eq!(elephc_proc_pipe_count(process), 1);
    assert_eq!(elephc_proc_pipe_index(process, 0), 1);
    (process, elephc_proc_pipe_fd(process, 0))
}

/// A shell command's stdout pipe carries its output and `proc_close()` its exit code.
#[test]
fn shell_command_output_and_exit_code() {
    let (process, stdout) = shell_with_stdout(c"echo hello; exit 3");
    assert_eq!(read_all(stdout), "hello\n");
    assert_eq!(elephc_proc_close(process), 3);
}

/// An argv command writes through a stdin pipe and reads the echo back.
#[test]
fn argv_command_round_trips_through_cat() {
    let builder = elephc_proc_builder_new();
    unsafe {
        assert_eq!(elephc_proc_builder_arg(builder, c"cat".as_ptr()), 1);
        assert_eq!(elephc_proc_builder_pipe(builder, 0, c"r".as_ptr()), 1);
        assert_eq!(elephc_proc_builder_pipe(builder, 1, c"w".as_ptr()), 1);
    }
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    let stdin = elephc_proc_pipe_fd(process, 0);
    let stdout = elephc_proc_pipe_fd(process, 1);
    let written = unsafe { libc::write(stdin as c_int, b"ping".as_ptr().cast(), 4) };
    assert_eq!(written, 4);
    unsafe { libc::close(stdin as c_int) };
    assert_eq!(read_all(stdout), "ping");
    assert_eq!(elephc_proc_close(process), 0);
}

/// `$cwd` and a replacement environment reach the child; unset names stay unset.
#[test]
fn cwd_and_replacement_environment() {
    let builder = elephc_proc_builder_new();
    unsafe {
        elephc_proc_builder_shell(builder, c"echo \"$A-${HOME:-none}\"; pwd".as_ptr());
        elephc_proc_builder_cwd(builder, c"/".as_ptr());
        elephc_proc_builder_env_reset(builder);
        elephc_proc_builder_env(builder, c"A=x".as_ptr());
        elephc_proc_builder_pipe(builder, 1, c"w".as_ptr());
    }
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    assert_eq!(read_all(elephc_proc_pipe_fd(process, 0)), "x-none\n/\n");
    assert_eq!(elephc_proc_close(process), 0);
}

/// A missing executable fails at spawn time with php-src's posix_spawn warning text.
#[test]
fn missing_executable_reports_posix_spawn_failure() {
    let builder = elephc_proc_builder_new();
    unsafe { elephc_proc_builder_arg(builder, c"elephc-proc-definitely-missing".as_ptr()) };
    assert_eq!(elephc_proc_spawn(builder), 0);
    assert_eq!(last_error(), "proc_open(): posix_spawn() failed: No such file or directory");
}

/// A redirect to an unnamed descriptor above 2 is php-src's "not found" warning.
#[test]
fn redirect_to_unknown_target_fails() {
    let builder = elephc_proc_builder_new();
    assert_eq!(elephc_proc_builder_redirect(builder, 2, 7), 0);
    assert_eq!(last_error(), "proc_open(): Redirection target 7 not found");
    elephc_proc_builder_free(builder);
}

/// `2 => ['redirect', 1]` merges stderr into the stdout pipe.
#[test]
fn redirect_merges_stderr_into_stdout() {
    let builder = elephc_proc_builder_new();
    unsafe {
        elephc_proc_builder_shell(builder, c"echo out; echo err >&2".as_ptr());
        elephc_proc_builder_pipe(builder, 1, c"w".as_ptr());
    }
    assert_eq!(elephc_proc_builder_redirect(builder, 2, 1), 1);
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    assert_eq!(read_all(elephc_proc_pipe_fd(process, 0)), "out\nerr\n");
    assert_eq!(elephc_proc_close(process), 0);
}

/// A file descriptor spec that cannot be opened reports php-src's stream warning.
#[test]
fn unopenable_file_reports_stream_warning() {
    let builder = elephc_proc_builder_new();
    let result = unsafe {
        elephc_proc_builder_file(builder, 0, c"/nonexistent/elephc".as_ptr(), c"r".as_ptr())
    };
    assert_eq!(result, 0);
    assert_eq!(
        last_error(),
        "proc_open(/nonexistent/elephc): Failed to open stream: No such file or directory"
    );
    elephc_proc_builder_free(builder);
}

/// A normal exit is cached by the first observation and reported again afterwards.
#[test]
fn normal_exit_is_cached() {
    let builder = elephc_proc_builder_new();
    unsafe { elephc_proc_builder_shell(builder, c"exit 4".as_ptr()) };
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    for _ in 0..200 {
        elephc_proc_poll(process);
        if elephc_proc_status_field(process, i64::from(FIELD_RUNNING)) == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_EXITCODE)), 4);
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_CACHED)), 1);
    elephc_proc_poll(process);
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_EXITCODE)), 4);
    assert_eq!(elephc_proc_close(process), 4);
}

/// A terminated child reports its signal once; `proc_close()` then answers -1.
#[test]
fn signal_death_is_reported_once() {
    let builder = elephc_proc_builder_new();
    unsafe { elephc_proc_builder_shell(builder, c"exec sleep 5".as_ptr()) };
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    assert_eq!(elephc_proc_terminate(process, i64::from(libc::SIGTERM)), 1);
    for _ in 0..200 {
        elephc_proc_poll(process);
        if elephc_proc_status_field(process, i64::from(FIELD_RUNNING)) == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_SIGNALED)), 1);
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_TERMSIG)), i64::from(libc::SIGTERM));
    assert_eq!(elephc_proc_status_field(process, i64::from(FIELD_CACHED)), 0);
    assert_eq!(elephc_proc_close(process), -1);
}

/// A signal death reaped by `proc_close()` itself returns the raw wait status.
#[test]
fn close_after_kill_returns_raw_status() {
    let builder = elephc_proc_builder_new();
    unsafe { elephc_proc_builder_shell(builder, c"kill -9 $$".as_ptr()) };
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    assert_eq!(elephc_proc_close(process), i64::from(libc::SIGKILL));
}

/// The status command is the shell text, or argv[0] for the array form.
#[test]
fn command_reports_shell_text_or_argv_zero() {
    let (process, stdout) = shell_with_stdout(c"echo a");
    let command = unsafe { CStr::from_ptr(elephc_proc_command(process)) };
    assert_eq!(command.to_str().unwrap(), "echo a");
    read_all(stdout);
    elephc_proc_close(process);
    let builder = elephc_proc_builder_new();
    unsafe {
        elephc_proc_builder_arg(builder, c"true".as_ptr());
        elephc_proc_builder_arg(builder, c"ignored".as_ptr());
    }
    let process = elephc_proc_spawn(builder);
    let command = unsafe { CStr::from_ptr(elephc_proc_command(process)) };
    assert_eq!(command.to_str().unwrap(), "true");
    assert_eq!(elephc_proc_close(process), 0);
}

/// `proc_close()` closes an unclosed stdin pipe first, so `cat` exits instead of hanging.
#[test]
fn close_closes_open_pipes_before_waiting() {
    let builder = elephc_proc_builder_new();
    unsafe {
        elephc_proc_builder_arg(builder, c"cat".as_ptr());
        elephc_proc_builder_pipe(builder, 0, c"r".as_ptr());
    }
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    let stdin = elephc_proc_pipe_fd(process, 0) as c_int;
    let spawned = inode(stdin);
    assert!(spawned.is_some());
    assert_eq!(elephc_proc_close(process), 0);
    assert_ne!(inode(stdin), spawned, "the pipe end must be released");
    // The number stays reserved for the PHP stream that owns it (now `/dev/null`), so a
    // later `fclose()` of that stream can never close an unrelated descriptor.
    let flags = unsafe { libc::fcntl(stdin, libc::F_GETFD) };
    assert!(flags >= 0, "the descriptor number must stay open");
    assert_ne!(flags & libc::FD_CLOEXEC, 0, "the placeholder must stay close-on-exec");
    unsafe { libc::close(stdin) };
}

/// A pipe end the program closed and whose number was reused is left alone.
#[test]
fn close_leaves_a_reused_descriptor_number_alone() {
    let builder = elephc_proc_builder_new();
    unsafe {
        elephc_proc_builder_shell(builder, c"exit 0".as_ptr());
        elephc_proc_builder_pipe(builder, 1, c"w".as_ptr());
    }
    let process = elephc_proc_spawn(builder);
    assert!(process > 0, "{}", last_error());
    let stdout = elephc_proc_pipe_fd(process, 0) as c_int;
    unsafe { libc::close(stdout) };
    let reused = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
    assert!(reused >= 0);
    // Another test thread may take the freed number first; never dup2 over its descriptor.
    let reused_same_number = reused == stdout;
    assert_eq!(elephc_proc_close(process), 0);
    if reused_same_number {
        assert!(unsafe { libc::fcntl(stdout, libc::F_GETFD) } >= 0, "a reused number must stay open");
    }
    unsafe { libc::close(reused) };
}
