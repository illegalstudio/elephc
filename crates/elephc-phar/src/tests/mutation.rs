//! Purpose:
//! Tests archive entry writes, updates, deletes, compression changes, and streams.
//!
//! Called from:
//! - `cargo test -p elephc-phar` through Rust's test harness.
//!
//! Key details:
//! - Mutations must preserve archive family, sibling entries, and existing compression.

use super::*;

/// Verifies native PHAR writes preserve existing entries and update duplicates.
#[test]
pub(super) fn writes_and_updates_native_phar_entries() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_put_entry_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
        Some(5)
    );
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
        Some(5)
    );
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"updated"),
        Some(7)
    );
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(
        extract_entry_bytes(&archive, b"one.txt").as_deref(),
        Some(&b"updated"[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies native PHAR writes preserve gzip compression on replaced entries.
#[test]
pub(super) fn writes_preserve_gzip_native_phar_entries() {
    let _policy = RequireHashGuard::relaxed();
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_gzip_update_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let original = b"gzip old payload gzip old payload";
    let stored = deflate_payload(original);
    let archive = build_native_phar_with_flags(&[(
        "z.txt",
        &stored,
        original.len() as u32,
        PHAR_FILE_MODE_0644 | PHAR_FLAG_GZIP,
    )]);
    std::fs::write(&path, archive).unwrap();
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"z.txt", b"gzip updated payload"),
        Some(20)
    );
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let entries = parse_native_phar_archive(&archive).unwrap().entries;
    assert_eq!(entries[0].compression, PharCompression::Gzip);
    assert_eq!(entries[0].payload, b"gzip updated payload");
}

/// Verifies native PHAR writes preserve bzip2 compression on replaced entries.
#[test]
pub(super) fn writes_preserve_bzip2_native_phar_entries() {
    let _policy = RequireHashGuard::relaxed();
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_bzip2_update_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let original = b"bzip2 old payload bzip2 old payload";
    let stored = bzip2_payload(original);
    let archive = build_native_phar_with_flags(&[(
        "b.txt",
        &stored,
        original.len() as u32,
        PHAR_FILE_MODE_0644 | PHAR_FLAG_BZIP2,
    )]);
    std::fs::write(&path, archive).unwrap();
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"bzip2 updated payload"),
        Some(21)
    );
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let entries = parse_native_phar_archive(&archive).unwrap().entries;
    assert_eq!(entries[0].compression, PharCompression::Bzip2);
    assert_eq!(entries[0].payload, b"bzip2 updated payload");
}

/// Verifies buffered PHAR stream descriptors keep concurrent payloads separate.
#[test]
pub(super) fn concurrent_phar_write_streams_preserve_distinct_entries() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_streams_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let path_bytes = path.to_string_lossy();
    let path_raw = path_bytes.as_bytes();
    let one = b"one.txt";
    let two = b"two.txt";
    let fd_one = unsafe {
        elephc_phar_stream_open_entry(path_raw.as_ptr(), path_raw.len(), one.as_ptr(), one.len())
    };
    let fd_two = unsafe {
        elephc_phar_stream_open_entry(path_raw.as_ptr(), path_raw.len(), two.as_ptr(), two.len())
    };
    assert_ne!(fd_one, usize::MAX);
    assert_ne!(fd_two, usize::MAX);
    assert_ne!(fd_one, fd_two);
    assert_eq!(
        unsafe { elephc_phar_stream_append(fd_two, b"bravo".as_ptr(), 5) },
        5
    );
    assert_eq!(
        unsafe { elephc_phar_stream_append(fd_one, b"alpha".as_ptr(), 5) },
        5
    );
    assert_eq!(elephc_phar_stream_finalize(fd_one), 1);
    assert_eq!(elephc_phar_stream_finalize(fd_two), 1);
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let entries = parse_native_phar_archive(&archive).unwrap().entries;
    assert_eq!(entry_payload(&entries, b"one.txt"), Some(b"alpha".as_slice()));
    assert_eq!(entry_payload(&entries, b"two.txt"), Some(b"bravo".as_slice()));
}

/// Verifies tar writes preserve the tar container family while updating entries.
#[test]
pub(super) fn writes_tar_entries() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_tar_write_{}_{}.tar",
        std::process::id(),
        "unit"
    ));
    std::fs::write(&path, build_tar(&[("one.txt", b"alpha")])).unwrap();
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
        Some(5)
    );
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(
        extract_entry_bytes(&archive, b"one.txt").as_deref(),
        Some(&b"alpha"[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
    assert_ne!(archive.get(0..5), Some(&b"<?php"[..]));
}

/// Verifies ZIP writes preserve the ZIP container family while updating entries.
#[test]
pub(super) fn writes_zip_entries() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_zip_write_{}_{}.zip",
        std::process::id(),
        "unit"
    ));
    std::fs::write(&path, build_zip(&[("one.txt", b"alpha", true)])).unwrap();
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
        Some(5)
    );
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(archive.get(0..4), Some(&[0x50, 0x4b, 0x03, 0x04][..]));
    assert_eq!(
        extract_entry_bytes(&archive, b"one.txt").as_deref(),
        Some(&b"alpha"[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies native PHAR deletion removes one entry while preserving siblings.
#[test]
pub(super) fn deletes_native_phar_entry_from_url() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_delete_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
        Some(5)
    );
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"two.txt", b"bravo"),
        Some(5)
    );
    let url = format!("phar://{}/one.txt", path.display());
    assert_eq!(delete_url_bytes(url.as_bytes()), Some(()));
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(extract_entry_bytes(&archive, b"one.txt"), None);
    assert_eq!(
        extract_entry_bytes(&archive, b"two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies tar and ZIP deletion preserve the archive family.
#[test]
pub(super) fn deletes_tar_and_zip_entries() {
    let tar_path = std::env::temp_dir().join(format!(
        "elephc_phar_delete_{}_{}.tar",
        std::process::id(),
        "unit"
    ));
    std::fs::write(&tar_path, build_tar(&[("one.txt", b"alpha"), ("two.txt", b"bravo")]))
        .unwrap();
    let tar_url = format!("phar://{}/one.txt", tar_path.display());
    assert_eq!(delete_url_bytes(tar_url.as_bytes()), Some(()));
    let tar_archive = std::fs::read(&tar_path).unwrap();
    std::fs::remove_file(&tar_path).ok();
    assert_eq!(extract_entry_bytes(&tar_archive, b"one.txt"), None);
    assert_eq!(
        extract_entry_bytes(&tar_archive, b"two.txt").as_deref(),
        Some(&b"bravo"[..])
    );

    let zip_path = std::env::temp_dir().join(format!(
        "elephc_phar_delete_{}_{}.zip",
        std::process::id(),
        "unit"
    ));
    std::fs::write(
        &zip_path,
        build_zip(&[("one.txt", b"alpha", false), ("two.txt", b"bravo", true)]),
    )
    .unwrap();
    let zip_url = format!("phar://{}/one.txt", zip_path.display());
    assert_eq!(delete_url_bytes(zip_url.as_bytes()), Some(()));
    let zip_archive = std::fs::read(&zip_path).unwrap();
    std::fs::remove_file(&zip_path).ok();
    assert_eq!(zip_archive.get(0..4), Some(&[0x50, 0x4b, 0x03, 0x04][..]));
    assert_eq!(extract_entry_bytes(&zip_archive, b"one.txt"), None);
    assert_eq!(
        extract_entry_bytes(&zip_archive, b"two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies deletion fails cleanly when the requested entry is absent.
#[test]
pub(super) fn delete_missing_entry_returns_none() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_delete_missing_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
        Some(5)
    );
    let url = format!("phar://{}/missing.txt", path.display());
    assert_eq!(delete_url_bytes(url.as_bytes()), None);
    std::fs::remove_file(&path).ok();
}

/// Verifies native PHAR archive-wide compression controls rewrite all entries.
#[test]
pub(super) fn sets_native_phar_archive_compression() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_compress_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let path_bytes = path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
        Some(5)
    );
    assert_eq!(
        put_entry_bytes(path_bytes.as_bytes(), b"two.txt", b"bravo"),
        Some(5)
    );
    assert_eq!(set_archive_compression(path_bytes.as_bytes(), 4_096), Some(()));
    let gzip_archive = std::fs::read(&path).unwrap();
    let gzip_entries = parse_native_phar_archive(&gzip_archive).unwrap().entries;
    assert!(gzip_entries
        .iter()
        .all(|entry| entry.compression == PharCompression::Gzip));
    assert_eq!(
        extract_entry_bytes(&gzip_archive, b"two.txt").as_deref(),
        Some(&b"bravo"[..])
    );

    assert_eq!(set_archive_compression(path_bytes.as_bytes(), 0), Some(()));
    let plain_archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let plain_entries = parse_native_phar_archive(&plain_archive).unwrap().entries;
    assert!(plain_entries
        .iter()
        .all(|entry| entry.compression == PharCompression::None));
    assert_eq!(
        extract_entry_bytes(&plain_archive, b"one.txt").as_deref(),
        Some(&b"alpha"[..])
    );
}

/// Verifies ZIP archive compression controls rewrite stored and deflated entries.
#[test]
pub(super) fn sets_zip_archive_compression() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_zip_compress_{}_{}.zip",
        std::process::id(),
        "unit"
    ));
    std::fs::write(
        &path,
        build_zip(&[
            ("one.txt", b"alpha alpha alpha", false),
            ("two.txt", b"bravo bravo bravo", false),
        ]),
    )
    .unwrap();
    let path_bytes = path.to_string_lossy();
    assert_eq!(set_archive_compression(path_bytes.as_bytes(), 4_096), Some(()));
    let deflated_archive = std::fs::read(&path).unwrap();
    let deflated_entries = parse_zip_archive(&deflated_archive).unwrap().entries;
    assert!(deflated_entries
        .iter()
        .all(|entry| entry.compression == PharCompression::Gzip));
    assert_eq!(
        extract_entry_bytes(&deflated_archive, b"two.txt").as_deref(),
        Some(&b"bravo bravo bravo"[..])
    );

    assert_eq!(set_archive_compression(path_bytes.as_bytes(), 0), Some(()));
    let stored_archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let stored_entries = parse_zip_archive(&stored_archive).unwrap().entries;
    assert!(stored_entries
        .iter()
        .all(|entry| entry.compression == PharCompression::None));
    assert_eq!(
        extract_entry_bytes(&stored_archive, b"one.txt").as_deref(),
        Some(&b"alpha alpha alpha"[..])
    );
}

/// Verifies compression controls reject unsupported constants and containers.
#[test]
pub(super) fn set_compression_rejects_unsupported_inputs() {
    let phar_path = std::env::temp_dir().join(format!(
        "elephc_phar_compress_bad_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let phar_bytes = phar_path.to_string_lossy();
    assert_eq!(
        put_entry_bytes(phar_bytes.as_bytes(), b"one.txt", b"alpha"),
        Some(5)
    );
    assert_eq!(set_archive_compression(phar_bytes.as_bytes(), 123), None);
    std::fs::remove_file(&phar_path).ok();

    let tar_path = std::env::temp_dir().join(format!(
        "elephc_phar_compress_bad_{}_{}.tar",
        std::process::id(),
        "unit"
    ));
    std::fs::write(&tar_path, build_tar(&[("one.txt", b"alpha")])).unwrap();
    let tar_bytes = tar_path.to_string_lossy();
    assert_eq!(set_archive_compression(tar_bytes.as_bytes(), 4_096), None);
    std::fs::remove_file(&tar_path).ok();

    let zip_path = std::env::temp_dir().join(format!(
        "elephc_phar_compress_bad_{}_{}.zip",
        std::process::id(),
        "unit"
    ));
    std::fs::write(&zip_path, build_zip(&[("one.txt", b"alpha", false)])).unwrap();
    let zip_bytes = zip_path.to_string_lossy();
    assert_eq!(set_archive_compression(zip_bytes.as_bytes(), 8_192), None);
    std::fs::remove_file(&zip_path).ok();
}

/// Verifies full phar:// URL writes split archive and entry names at run time.
#[test]
pub(super) fn writes_native_phar_entries_from_url() {
    let path = std::env::temp_dir().join(format!(
        "elephc_phar_put_url_{}_{}.phar",
        std::process::id(),
        "unit"
    ));
    let url = format!("phar://{}/one.txt", path.display());
    assert_eq!(put_url_bytes(url.as_bytes(), b"alpha"), Some(5));
    let nested_url = format!("phar://{}/dir/two.txt", path.display());
    assert_eq!(put_url_bytes(nested_url.as_bytes(), b"bravo"), Some(5));
    let archive = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(
        extract_entry_bytes(&archive, b"one.txt").as_deref(),
        Some(&b"alpha"[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}
