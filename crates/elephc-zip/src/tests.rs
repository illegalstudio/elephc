//! Purpose:
//! Archive-engine unit tests for creation, mutation, encryption, extraction, and reopen paths.
//!
//! Called from:
//! - `cargo test -p elephc-zip` through Rust's test harness.
//!
//! Key details:
//! - Each test uses an isolated temporary directory; no PHP compiler is involved.
//! - Assertions exercise the same model methods the C ABI calls.

use std::path::{Path, PathBuf};

use crate::consts::*;
use crate::error::ZipError;
use crate::model::Archive;

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
    let mut archive = Archive::open(path.clone(), CREATE).unwrap();
    archive.add_from_string(b"hello.txt", b"hello world".to_vec(), FL_OVERWRITE).unwrap();
    archive.add_directory(b"dir", 0).unwrap();
    archive.close().unwrap();
    assert!(Path::new(&path).is_file());

    let mut reopened = Archive::open(path, 0).unwrap();
    assert_eq!(reopened.num_entries(), 2);
    assert_eq!(reopened.payload(0, 0, None).unwrap(), b"hello world");
    assert_eq!(reopened.entries[1].name, b"dir/");
    reopened.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies duplicate adds fail without `FL_OVERWRITE`, and overwrite keeps the index.
#[test]
fn overwrite_reuses_the_original_index() {
    let dir = test_dir("overwrite");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path, CREATE).unwrap();
    assert_eq!(archive.add_from_string(b"a.txt", b"one".to_vec(), FL_OVERWRITE).unwrap(), 0);
    assert_eq!(archive.add_from_string(b"a.txt", b"two".to_vec(), 0), Err(ZipError::Exists));
    assert_eq!(archive.add_from_string(b"a.txt", b"three".to_vec(), FL_OVERWRITE).unwrap(), 0);
    assert_eq!(archive.num_entries(), 1);
    assert_eq!(archive.payload(0, 0, None).unwrap(), b"three");
    archive.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies rename, delete, comment, and mtime mutations survive a close.
#[test]
fn mutations_survive_reopen() {
    let dir = test_dir("mutations");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).unwrap();
    archive.add_from_string(b"one.txt", b"1".to_vec(), FL_OVERWRITE).unwrap();
    archive.add_from_string(b"two.txt", b"2".to_vec(), FL_OVERWRITE).unwrap();
    archive.rename(0, b"renamed.txt").unwrap();
    archive.set_comment(0, b"note").unwrap();
    archive.set_mtime(0, 1_600_000_000).unwrap();
    archive.delete(1).unwrap();
    archive.set_archive_comment(b"archive note").unwrap();
    archive.close().unwrap();

    let mut reopened = Archive::open(path, 0).unwrap();
    assert_eq!(reopened.num_entries(), 1);
    assert_eq!(reopened.entries[0].name, b"renamed.txt");
    assert_eq!(reopened.entries[0].comment, b"note");
    assert_eq!(reopened.entries[0].mtime, 1_600_000_000);
    assert_eq!(reopened.comment, b"archive note");
    reopened.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies AES-256 encryption requires the password and round-trips through reopen.
#[test]
fn aes_encryption_requires_the_password() {
    let dir = test_dir("encryption");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).unwrap();
    archive.add_from_string(b"secret.txt", b"classified".to_vec(), FL_OVERWRITE).unwrap();
    archive.set_encryption(0, EM_AES_256, Some(b"hunter2".to_vec())).unwrap();
    archive.close().unwrap();

    let mut reopened = Archive::open(path, 0).unwrap();
    assert_eq!(reopened.entries[0].encryption, EM_AES_256);
    assert_eq!(reopened.payload(0, 0, None).unwrap_err(), ZipError::NoPassword);
    assert_eq!(reopened.payload(0, 0, Some(b"wrong")).unwrap_err(), ZipError::WrongPassword);
    assert_eq!(reopened.payload(0, 0, Some(b"hunter2")).unwrap(), b"classified");
    reopened.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies extraction creates nested directories and restores file contents.
#[test]
fn extract_creates_nested_directories() {
    let dir = test_dir("extract");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path, CREATE).unwrap();
    archive.add_directory(b"outer/inner", 0).unwrap();
    archive.add_from_string(b"outer/file.txt", b"payload".to_vec(), FL_OVERWRITE).unwrap();
    archive.close().unwrap();

    let mut archive = Archive::open(archive_path(&dir, "test.zip"), 0).unwrap();
    let destination = dir.join("out");
    crate::extract::extract_to(&mut archive, destination.to_str().unwrap(), None).unwrap();
    assert!(destination.join("outer/inner").is_dir());
    assert_eq!(std::fs::read(destination.join("outer/file.txt")).unwrap(), b"payload");
    archive.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies traditional PKWARE encryption writes and reads through reopen.
#[test]
fn zipcrypto_write_requires_the_password() {
    let dir = test_dir("zipcrypto");
    let path = archive_path(&dir, "test.zip");
    let mut archive = Archive::open(path.clone(), CREATE).unwrap();
    archive.add_from_string(b"legacy.txt", b"legacy payload".to_vec(), FL_OVERWRITE).unwrap();
    archive.set_encryption(0, EM_TRAD_PKWARE, Some(b"secret".to_vec())).unwrap();
    archive.close().unwrap();

    let mut reopened = Archive::open(path, 0).unwrap();
    assert_eq!(reopened.entries[0].encryption, EM_TRAD_PKWARE);
    assert_eq!(reopened.payload(0, 0, None).unwrap_err(), ZipError::NoPassword);
    assert_eq!(reopened.payload(0, 0, Some(b"wrong")).unwrap_err(), ZipError::WrongPassword);
    assert_eq!(reopened.payload(0, 0, Some(b"secret")).unwrap(), b"legacy payload");
    reopened.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
