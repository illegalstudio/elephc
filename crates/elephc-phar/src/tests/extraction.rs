//! Purpose:
//! Tests extraction and entry listing across supported PHAR container families.
//!
//! Called from:
//! - `cargo test -p elephc-phar` through Rust's test harness.
//!
//! Key details:
//! - Native PHAR, tar, stored ZIP, and deflated ZIP fixtures share canonical expectations.

use super::*;

/// Verifies native PHAR manifest extraction.
#[test]
pub(super) fn extracts_native_phar_entry() {
    let archive = build_native_phar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies tar container extraction.
#[test]
pub(super) fn extracts_tar_entry() {
    let archive = build_tar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
    assert_eq!(
        extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
        Some(&b"bravo"[..])
    );
}

/// Verifies ZIP store and deflate extraction.
#[test]
pub(super) fn extracts_zip_entries() {
    let archive = build_zip(&[
        ("plain.txt", b"stored", false),
        ("deflated.txt", b"deflated payload", true),
    ]);
    assert_eq!(
        extract_entry_bytes(&archive, b"plain.txt").as_deref(),
        Some(&b"stored"[..])
    );
    assert_eq!(
        extract_entry_bytes(&archive, b"deflated.txt").as_deref(),
        Some(&b"deflated payload"[..])
    );
}

/// Verifies entry-name listing across supported archive families.
#[test]
pub(super) fn lists_entry_names_for_supported_archive_families() {
    let base = std::env::temp_dir().join(format!(
        "elephc_phar_list_{}_{}",
        std::process::id(),
        "unit"
    ));
    let phar_path = base.with_extension("phar");
    let tar_path = base.with_extension("tar");
    let zip_path = base.with_extension("zip");

    std::fs::write(
        &phar_path,
        build_native_phar(&[("one.txt", b"alpha"), ("dir/two.txt", b"bravo")]),
    )
    .unwrap();
    std::fs::write(
        &tar_path,
        build_tar(&[("tar.txt", b"tar"), ("dir/nested.txt", b"nested")]),
    )
    .unwrap();
    std::fs::write(
        &zip_path,
        build_zip(&[("zip.txt", b"zip", false), ("def.txt", b"def", true)]),
    )
    .unwrap();

    assert_eq!(
        entry_names_bytes(phar_path.to_string_lossy().as_bytes()).as_deref(),
        Some(serialized_names(&["one.txt", "dir/two.txt"]).as_slice())
    );
    assert_eq!(
        entry_names_bytes(tar_path.to_string_lossy().as_bytes()).as_deref(),
        Some(serialized_names(&["tar.txt", "dir/nested.txt"]).as_slice())
    );
    assert_eq!(
        entry_names_bytes(zip_path.to_string_lossy().as_bytes()).as_deref(),
        Some(serialized_names(&["zip.txt", "def.txt"]).as_slice())
    );

    std::fs::remove_file(&phar_path).ok();
    std::fs::remove_file(&tar_path).ok();
    std::fs::remove_file(&zip_path).ok();
}

/// Each thread's published result stays alive until that thread has copied it
/// out. Two threads publishing *differently sized* payloads at the same moment
/// must each read back their own: with one process-wide buffer, the second
/// thread's `publish_result` clears and refills it — reallocating away the bytes
/// the first thread was just handed a pointer into. (`elephc-pdo` shipped this
/// exact bug and it reached CI as non-UTF-8 garbage where a SQLSTATE belonged.)
///
/// A thread republishing on its OWN thread legitimately invalidates its own
/// pointer — that is the documented contract — so each thread publishes exactly
/// once and the only interference is the other thread's.
#[test]
pub(super) fn extract_buffers_are_isolated_between_threads() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // Distinct fill bytes as well as distinct sizes, so a cross-thread read is
    // detectable whichever thread publishes last.
    let handles = [(1_usize, 0xAA_u8), (4096_usize, 0x55_u8)].map(|(size, fill)| {
        let barrier = std::sync::Arc::clone(&barrier);
        std::thread::spawn(move || {
            let expected = vec![fill; size];
            let mut out_len: usize = 0;

            barrier.wait();
            let pointer = publish_result(expected.clone(), &mut out_len);
            assert!(!pointer.is_null(), "publish must return a pointer");

            // Both threads have filled the buffer and taken their pointer before
            // either reads through it: that is the window in which one
            // process-wide buffer frees the bytes the other thread points at.
            barrier.wait();
            assert_eq!(
                out_len,
                expected.len(),
                "published length was overwritten by the other thread"
            );
            let actual = unsafe { std::slice::from_raw_parts(pointer, expected.len()) }.to_vec();
            assert_eq!(
                actual, expected,
                "extract buffer was overwritten by the other thread"
            );
        })
    });

    for handle in handles {
        handle.join().expect("publish worker must not panic");
    }
}
