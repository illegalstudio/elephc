//! Purpose:
//! Archive-engine unit tests for creation, mutation, encryption, extraction, and reopen
//! paths against the real libzip 1.11.4 archives.
//!
//! Called from:
//! - `cargo test -p elephc-zip` with `ELEPHC_ZIP_NATIVE_LIB_DIRS` set.
//!
//! Key details:
//! - Each test uses an isolated temporary directory; no PHP compiler is involved.
//! - Assertions exercise the same `Archive` methods the C ABI calls.

use std::path::{Path, PathBuf};

use crate::archive::Archive;
use crate::consts::*;
use crate::error::ZipError;

/// Creates and returns one isolated test directory.
fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("elephc-zip-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test directory");
    dir
}

/// Joins a test directory and archive name into a lossy path string.
fn archive_path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// Creates, writes, reopens, and reads a tiny archive.
#[test]
fn create_write_reopen_roundtrip() {
    let dir = test_dir("roundtrip");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).expect("create archive");
    archive.add_from_string(b"hello.txt", b"hello world".to_vec(), FL_OVERWRITE).expect("add file");
    archive.add_directory(b"dir", 0).expect("add directory");
    archive.close().expect("close");
    assert!(Path::new(&path).is_file());

    let mut reopened = Archive::open(path, 0).expect("reopen");
    assert_eq!(reopened.num_entries(), 2);
    assert_eq!(reopened.payload(0, 0).expect("payload"), b"hello world");
    assert_eq!(reopened.name(1, 0).expect("name"), b"dir/");
    reopened.close().expect("close reopened");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies duplicate adds fail without `FL_OVERWRITE`, and overwrite keeps the index.
#[test]
fn overwrite_reuses_the_original_index() {
    let dir = test_dir("overwrite");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path, CREATE).expect("create archive");
    assert_eq!(archive.add_from_string(b"a.txt", b"one".to_vec(), FL_OVERWRITE).expect("first"), 0);
    assert_eq!(
        archive.add_from_string(b"a.txt", b"two".to_vec(), 0),
        Err(ZipError::Exists)
    );
    assert_eq!(archive.add_from_string(b"a.txt", b"three".to_vec(), FL_OVERWRITE).expect("overwrite"), 0);
    assert_eq!(archive.num_entries(), 1);
    assert_eq!(archive.payload(0, 0).expect("payload"), b"three");
    archive.close().expect("close");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies rename, delete, comment, and mtime mutations survive a close.
#[test]
fn mutations_survive_reopen() {
    let dir = test_dir("mutations");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).expect("create archive");
    archive.add_from_string(b"one.txt", b"1".to_vec(), FL_OVERWRITE).expect("first");
    archive.add_from_string(b"two.txt", b"2".to_vec(), FL_OVERWRITE).expect("second");
    archive.rename(0, b"renamed.txt").expect("rename");
    archive.set_comment(0, b"note").expect("comment");
    archive.set_mtime(0, 1_600_000_000, 0).expect("mtime");
    archive.delete(1).expect("delete");
    archive.set_archive_comment(b"archive note").expect("archive comment");
    archive.close().expect("close");

    let mut reopened = Archive::open(path, 0).expect("reopen");
    assert_eq!(reopened.num_entries(), 1);
    assert_eq!(reopened.name(0, 0).expect("name"), b"renamed.txt");
    assert_eq!(reopened.comment(0, 0).expect("comment"), b"note");
    assert_eq!(reopened.stat(0, 0).expect("stat").mtime, 1_600_000_000);
    assert_eq!(reopened.archive_comment(0).expect("archive comment"), b"archive note");
    reopened.close().expect("close reopened");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies AES-256 encryption requires the password and round-trips through reopen.
#[test]
fn aes_encryption_requires_the_password() {
    let dir = test_dir("encryption");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).expect("create archive");
    archive.add_from_string(b"secret.txt", b"classified".to_vec(), FL_OVERWRITE).expect("add");
    archive.set_encryption(0, EM_AES_256, Some(b"hunter2")).expect("encrypt");
    archive.close().expect("close");

    let mut reopened = Archive::open(path, 0).expect("reopen");
    assert_eq!(reopened.stat(0, 0).expect("stat").encryption_method, EM_AES_256 as u16);
    assert_eq!(reopened.payload(0, 0), Err(ZipError::NoPassword));
    reopened.set_password(b"wrong").expect("set wrong password");
    assert!(reopened.payload(0, 0).is_err());
    reopened.set_password(b"hunter2").expect("set password");
    assert_eq!(reopened.payload(0, 0).expect("payload"), b"classified");
    reopened.close().expect("close reopened");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies extraction creates nested directories and restores file contents.
#[test]
fn extract_creates_nested_directories() {
    let dir = test_dir("extract");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path, CREATE).expect("create archive");
    archive.add_directory(b"outer/inner", 0).expect("add directory");
    archive.add_from_string(b"outer/file.txt", b"payload".to_vec(), FL_OVERWRITE).expect("add file");
    archive.close().expect("close");

    let mut archive = Archive::open(archive_path(&dir, "test.zip"), 0).expect("reopen");
    let destination = dir.join("out");
    crate::extract::extract_to(&archive, destination.to_str().expect("utf-8 destination"), None)
        .expect("extract");
    assert!(destination.join("outer/inner").is_dir());
    assert_eq!(std::fs::read(destination.join("outer/file.txt")).expect("read"), b"payload");
    archive.close().expect("close");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies traditional PKWARE encryption writes and reads through reopen.
#[test]
fn zipcrypto_write_requires_the_password() {
    let dir = test_dir("zipcrypto");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).expect("create archive");
    archive.add_from_string(b"legacy.txt", b"legacy payload".to_vec(), FL_OVERWRITE).expect("add");
    archive.set_encryption(0, EM_TRAD_PKWARE, Some(b"secret")).expect("encrypt");
    archive.close().expect("close");

    let mut reopened = Archive::open(path, 0).expect("reopen");
    assert_eq!(reopened.stat(0, 0).expect("stat").encryption_method, EM_TRAD_PKWARE as u16);
    assert_eq!(reopened.payload(0, 0), Err(ZipError::NoPassword));
    reopened.set_password(b"wrong").expect("set wrong password");
    assert!(reopened.payload(0, 0).is_err());
    reopened.set_password(b"secret").expect("set password");
    assert_eq!(reopened.payload(0, 0).expect("payload"), b"legacy payload");
    reopened.close().expect("close reopened");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies the bridge reports the linked libzip release, not a hardcoded string.
#[test]
fn linked_libzip_version_matches_the_pinned_catalog_release() {
    let pointer = unsafe { crate::ffi::zip_libzip_version() };
    assert!(!pointer.is_null());
    let version = unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy();
    assert_eq!(version, "1.11.4");
}
