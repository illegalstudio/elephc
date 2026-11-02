//! Purpose:
//! Unit tests for the process spawner and the wait-status table.
//!
//! Called from:
//! - `cargo test -p elephc-proc`.
//!
//! Key details:
//! - Tests spawn real `/bin/sh` children; each uses a distinct table token so the shared table
//!   never couples two tests.

use crate::spawn::{spawn, Request, Source};
use crate::table;
use std::ffi::CString;
use std::io::Read;
use std::os::fd::FromRawFd;

/// Builds a request for a shell command line.
fn shell(command: &str) -> Request {
    Request {
        shell: true,
        args: vec![CString::new(command).unwrap()],
        cwd: None,
        env: None,
        descriptors: Vec::new(),
    }
}

/// Reads a parent pipe end to EOF.
fn drain(fd: i32) -> String {
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    text
}

/// Verifies a shell child writes through a pipe and exits with its status.
#[test]
fn shell_command_pipes_stdout_and_reports_exit_code() {
    let mut request = shell("echo hi; exit 3");
    request.descriptors.push((1, Source::Pipe { child_reads: false }));
    let spawned = spawn(&request).expect("spawn");
    assert_eq!(drain(spawned.pipes[0].1), "hi\n");
    table::register(9001, spawned.pid, "echo hi; exit 3", &[]);
    assert_eq!(table::close(9001), Some(3));
}

/// Verifies the array form runs without a shell and sees the replacement environment and cwd.
#[test]
fn array_command_uses_cwd_and_replacement_environment() {
    let request = Request {
        shell: false,
        args: vec![CString::new("sh").unwrap(), CString::new("-c").unwrap(),
            CString::new("echo $FOO:$(pwd)").unwrap()],
        cwd: Some(CString::new("/").unwrap()),
        env: Some(vec![CString::new("FOO=bar").unwrap(), CString::new("PATH=/usr/bin:/bin").unwrap()]),
        descriptors: vec![(1, Source::Pipe { child_reads: false })],
    };
    let spawned = spawn(&request).expect("spawn");
    assert_eq!(drain(spawned.pipes[0].1), "bar:/\n");
    table::register(9002, spawned.pid, "sh", &[]);
    assert_eq!(table::close(9002), Some(0));
}

/// Verifies a missing program is reported as ENOENT and leaves no child behind.
#[test]
fn missing_program_reports_enoent() {
    let request = Request {
        shell: false,
        args: vec![CString::new("/nonexistent/elephc-proc-test").unwrap()],
        cwd: None,
        env: None,
        descriptors: vec![(1, Source::Pipe { child_reads: false })],
    };
    assert_eq!(spawn(&request).err(), Some(libc::ENOENT));
}

/// Verifies stdin pipes, redirects, and file descriptors compose.
#[test]
fn stdin_pipe_and_stderr_redirect() {
    let mut request = shell("cat; echo err >&2");
    request.descriptors.push((0, Source::Pipe { child_reads: true }));
    request.descriptors.push((1, Source::Pipe { child_reads: false }));
    request.descriptors.push((2, Source::Redirect(1)));
    let spawned = spawn(&request).expect("spawn");
    let stdin = spawned.pipes.iter().find(|(child, _)| *child == 0).unwrap().1;
    let stdout = spawned.pipes.iter().find(|(child, _)| *child == 1).unwrap().1;
    unsafe {
        libc::write(stdin, b"abc".as_ptr() as *const libc::c_void, 3);
        libc::close(stdin);
    }
    assert_eq!(drain(stdout), "abcerr\n");
    table::register(9003, spawned.pid, "cat", &[]);
    assert_eq!(table::close(9003), Some(0));
}

/// Verifies descriptor targets that collide with source numbers still land correctly.
#[test]
fn high_descriptor_targets_do_not_clobber_sources() {
    let mut request = shell("echo three >&3");
    request.descriptors.push((3, Source::Pipe { child_reads: false }));
    let spawned = spawn(&request).expect("spawn");
    assert_eq!(drain(spawned.pipes[0].1), "three\n");
    table::register(9004, spawned.pid, "echo", &[]);
    assert_eq!(table::close(9004), Some(0));
}

/// Verifies status caching: a normal exit is cached, a signal death is not.
#[test]
fn status_caches_normal_exit_only() {
    let spawned = spawn(&shell("exit 5")).expect("spawn");
    table::register(9005, spawned.pid, "exit 5", &[]);
    let mut snapshot = table::poll(9005).unwrap();
    for _ in 0..200 {
        if !snapshot.running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        snapshot = table::poll(9005).unwrap();
    }
    assert!(!snapshot.running && snapshot.cached && snapshot.exitcode == 5);
    assert_eq!(table::poll(9005).unwrap().exitcode, 5);
    assert_eq!(table::close(9005), Some(5));

    let spawned = spawn(&shell("sleep 30")).expect("spawn");
    table::register(9006, spawned.pid, "sleep 30", &[]);
    assert!(table::poll(9006).unwrap().running);
    assert_eq!(table::terminate(9006, libc::SIGTERM), Some(true));
    let mut snapshot = table::poll(9006).unwrap();
    for _ in 0..200 {
        if !snapshot.running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        snapshot = table::poll(9006).unwrap();
    }
    assert!(snapshot.signaled && snapshot.termsig == libc::SIGTERM as i64 && !snapshot.cached);
    let again = table::poll(9006).unwrap();
    assert!(!again.running && !again.signaled);
    assert_eq!(table::close(9006), Some(-1));
}
