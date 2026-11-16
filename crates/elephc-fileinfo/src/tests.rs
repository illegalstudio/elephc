//! Purpose:
//! Tests native fileinfo lifetime, flag restoration and thread-local result storage.
//!
//! Called from:
//! - Bridge unit tests with ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR set.
//!
//! Key details:
//! - Every test uses the managed embedded database, never the host database.

use super::*;

/// Real socket metadata follows PHP's directory-bit rule without reading a device or pathname.
#[test]
fn fileinfo_native_php_directory_mode() {
    let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let owned: std::os::fd::OwnedFd = socket.into();
    let file = std::fs::File::from(owned);
    assert!(directory_mode(file.metadata().unwrap().mode()));
    assert!(directory_mode(0o060600));
    assert!(!directory_mode(0o100644));
    assert!(!directory_mode(0o020666));
}

/// Creates a MIME handle backed by the embedded database.
unsafe fn open() -> i64 {
    let handle = elephc_fileinfo_open(16, c"".as_ptr());
    assert_ne!(handle, 0, "{}", CStr::from_ptr(elephc_fileinfo_error()).to_string_lossy());
    handle
}

/// A binary buffer is length-delimited and a per-call encoding override is temporary.
#[test]
fn fileinfo_native_binary_and_flags() {
    unsafe {
        let h = open();
        let data = b"hello\0binary\0tail";
        assert_eq!(CStr::from_ptr(elephc_fileinfo_buffer(h, data.as_ptr().cast(), data.len() as i64, 1024)).to_bytes(), b"binary");
        assert_eq!(CStr::from_ptr(elephc_fileinfo_buffer(h, c"hello".as_ptr(), 5, 0)).to_bytes(), b"text/plain");
        assert_eq!(elephc_fileinfo_free(h), 1);
        assert_eq!(elephc_fileinfo_free(h), 1);
    }
}

/// A failed file query restores the persistent flags before a subsequent buffer query.
#[test]
fn fileinfo_native_failure_restores_flags() {
    unsafe {
        let h = open();
        elephc_fileinfo_file(h, c"/elephc-fileinfo-missing/file".as_ptr(), 1024);
        assert_eq!(elephc_fileinfo_ok(), 0);
        assert_eq!(elephc_fileinfo_open_failed(), 1);
        assert_eq!(CStr::from_ptr(elephc_fileinfo_error()).to_bytes(), b"No such file or directory");
        assert_eq!(CStr::from_ptr(elephc_fileinfo_buffer(h, c"hello".as_ptr(), 5, 0)).to_bytes(), b"text/plain");
        assert_eq!(elephc_fileinfo_open_failed(), 0);
        elephc_fileinfo_free(h);
    }
}

/// Fileinfo copies transient TLS detail and removes Rust's errno display suffix.
#[test]
fn fileinfo_native_network_error_detail() {
    unsafe {
        let source = CString::new("Connection refused (os error 111)").unwrap();
        let pointer = elephc_fileinfo_network_error(0, source.as_ptr());
        drop(source);
        assert_eq!(CStr::from_ptr(pointer).to_bytes(), b"Connection refused");
        assert_eq!(CStr::from_ptr(elephc_fileinfo_network_error(2, c"".as_ptr())).to_bytes(), b"No such file or directory");
        assert_eq!(CStr::from_ptr(elephc_fileinfo_network_error(0, c"".as_ptr())).to_bytes(), b"operation failed");
    }
}

/// Independent owned reads preserve binary bytes and remain valid across a reentrant read.
#[test]
fn fileinfo_native_temporary_read_lifetime() {
    use std::time::{SystemTime, UNIX_EPOCH};
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        /// Removes only the two fixture files and the directory this test successfully created.
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.join("first"));
            let _ = std::fs::remove_file(self.0.join("second"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("elephc-fileinfo-read-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    std::fs::write(directory.0.join("first"), b"HELLO\0binary").unwrap();
    std::fs::write(directory.0.join("second"), b"other").unwrap();
    let first = CString::new(directory.0.join("first").as_os_str().as_bytes()).unwrap();
    let second = CString::new(directory.0.join("second").as_os_str().as_bytes()).unwrap();
    unsafe {
        let directory_path = CString::new(directory.0.as_os_str().as_bytes()).unwrap();
        assert_eq!(elephc_fileinfo_read(directory_path.as_ptr()), -1);
        let one = elephc_fileinfo_read(first.as_ptr());
        assert_ne!(one, 0);
        let pointer = elephc_fileinfo_read_data(one).cast::<u8>();
        let two = elephc_fileinfo_read(second.as_ptr());
        assert_ne!(two, 0);
        assert_eq!(elephc_fileinfo_read_length(one), 12);
        assert_eq!(std::slice::from_raw_parts(pointer, 12), b"HELLO\0binary");
        elephc_fileinfo_read_release(two);
        assert_eq!(std::slice::from_raw_parts(pointer, 12), b"HELLO\0binary");
        elephc_fileinfo_read_release(one);
        assert_eq!(elephc_fileinfo_read_length(one), 0);
        elephc_fileinfo_read_release(one);
    }
}

/// Directory results require a successful open and ignore MIME flags after descriptor stat.
#[test]
fn fileinfo_native_directory_open_permissions() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        /// Restores access before removing exactly this test's empty directory.
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let directory = Directory(std::env::temp_dir().join(format!("elephc-fileinfo-dir-{}-{nonce}", std::process::id())));
    std::fs::create_dir(&directory.0).unwrap();
    std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0)).unwrap();
    let can_open = std::fs::File::open(&directory.0).is_ok();
    let path = CString::new(directory.0.as_os_str().as_bytes()).unwrap();
    unsafe {
        let h = open();
        let result = elephc_fileinfo_file(h, path.as_ptr(), 16);
        assert_eq!(elephc_fileinfo_ok(), i64::from(can_open));
        if can_open {
            assert_eq!(CStr::from_ptr(result).to_bytes(), b"directory");
        } else {
            assert_eq!(elephc_fileinfo_open_failed(), 1);
        }
        std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(CStr::from_ptr(elephc_fileinfo_file(h, path.as_ptr(), 16)).to_bytes(), b"directory");
        assert_eq!(elephc_fileinfo_ok(), 1);
        elephc_fileinfo_free(h);
    }
}

/// Another thread cannot invalidate a returned pointer while the first thread reads it.
#[test]
fn fileinfo_native_results_are_thread_local() {
    unsafe {
        let h = open();
        let result = elephc_fileinfo_buffer(h, c"hello".as_ptr(), 5, 0);
        std::thread::spawn(|| {
            let other = open();
            elephc_fileinfo_buffer(other, c"other".as_ptr(), 5, 1024);
            elephc_fileinfo_free(other);
        }).join().unwrap();
        assert_eq!(CStr::from_ptr(result).to_bytes(), b"text/plain");
        elephc_fileinfo_free(h);
    }
}

/// A FIFO waiting for a writer cannot stall queries or frees on independent handles.
#[test]
fn fileinfo_native_blocked_file_does_not_block_other_handles() {
    use std::io::Write;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        /// Removes only this test's FIFO and its uniquely created parent directory.
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let directory = Directory(std::env::temp_dir().join(format!("elephc-fileinfo-{}-{nonce}", std::process::id())));
    std::fs::create_dir(&directory.0).unwrap();
    let fifo = directory.0.join("input");
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    unsafe {
        let blocked = open();
        let peer = open();
        let (started, waiting) = mpsc::channel();
        let query = std::thread::spawn(move || {
            started.send(()).unwrap();
            elephc_fileinfo_file(blocked, path.as_ptr(), 0);
            let succeeded = elephc_fileinfo_ok();
            elephc_fileinfo_free(blocked);
            succeeded
        });
        waiting.recv().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let (finished, result) = mpsc::channel();
        let independent = std::thread::spawn(move || {
            let flags = elephc_fileinfo_set_flags(peer, 1024);
            let value = CStr::from_ptr(elephc_fileinfo_buffer(peer, c"hello".as_ptr(), 5, 0)).to_owned();
            let freed = elephc_fileinfo_free(peer);
            finished.send((freed, flags, value)).unwrap();
        });
        let completed = result.recv_timeout(Duration::from_secs(5));
        // Always release the FIFO before asserting, including on the old global-lock path.
        let mut writer = std::fs::File::create(&fifo).unwrap();
        // libmagic may classify a FIFO without reading, closing its reader first.
        let _ = writer.write_all(b"hello");
        drop(writer);
        assert_eq!(query.join().unwrap(), 1);
        independent.join().unwrap();
        let (freed, flags, value) = completed.expect("an unrelated fileinfo handle was blocked by the FIFO");
        assert_eq!((freed, flags, value.to_bytes()), (1, 1, b"us-ascii".as_slice()));
    }
}
