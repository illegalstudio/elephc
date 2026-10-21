//! Purpose:
//! Verify upload-progress counters survive session replacement and body relocation
//! across multiple frames and file parts, the lifetime boundaries in PHP's tracker UAF.
//!
//! Called from:
//! - `crate::session::upload_progress::tests` through the elephc-web unit-test harness.
//!
//! Key details:
//! - Exercise the real file RMW for all three session serializers and cleanup modes.
//! - Expected bytes are independent fixtures, not the production progress serializer.
//! - Each test owns its directory and never changes process-global session config.

use super::incremental_tracker;
use super::super::Handler;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct SessionDirectory(PathBuf);

impl SessionDirectory {
    /// Creates a unique private fixture directory without changing global configuration.
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "elephc-progress-reload-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for SessionDirectory {
    /// Removes the session fixture even when a regression assertion panics.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Frames ordered serialized pairs without using production session-edit helpers.
fn session_fixture(handler: Handler, pairs: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    if handler == Handler::PhpSerialize {
        out.extend_from_slice(format!("a:{}:{{", pairs.len()).as_bytes());
    }
    for (key, value) in pairs {
        match handler {
            Handler::Php => out.extend_from_slice(format!("{key}|").as_bytes()),
            Handler::PhpBinary => {
                out.push(key.len() as u8);
                out.extend_from_slice(key.as_bytes());
            }
            Handler::PhpSerialize => {
                out.extend_from_slice(format!("s:{}:\"{key}\";", key.len()).as_bytes());
            }
        }
        out.extend_from_slice(value);
    }
    if handler == Handler::PhpSerialize {
        out.push(b'}');
    }
    out
}

/// Constructs the expected snapshot from explicit per-frame counters and file states.
fn progress_fixture(bytes: usize, done: bool, files: &[(&str, usize, bool)]) -> Vec<u8> {
    let mut value = format!(
        concat!(
            "a:5:{{s:10:\"start_time\";i:0;s:14:\"content_length\";i:100;",
            "s:15:\"bytes_processed\";i:{bytes};s:4:\"done\";b:{done};",
            "s:5:\"files\";a:{count}:{{",
        ),
        bytes = bytes,
        done = usize::from(done),
        count = files.len(),
    );
    for (index, (name, size, done)) in files.iter().enumerate() {
        value.push_str(&format!(
            concat!(
                "i:{index};a:7:{{s:10:\"field_name\";s:1:\"f\";",
                "s:4:\"name\";s:{len}:\"{name}\";s:8:\"tmp_name\";s:0:\"\";",
                "s:5:\"error\";i:0;s:4:\"done\";b:{done};s:10:\"start_time\";i:0;",
                "s:15:\"bytes_processed\";i:{size};}}",
            ),
            index = index,
            len = name.len(),
            name = name,
            done = usize::from(*done),
            size = size,
        ));
    }
    value.push_str("}}");
    value.into_bytes()
}

/// Replaces the disk session between writes while tracking two files over several frames.
fn exercise_session_reloads(cleanup: bool) {
    for handler in [Handler::Php, Handler::PhpBinary, Handler::PhpSerialize] {
        let directory = SessionDirectory::new();
        let path = directory.0.join("sess_sid123");
        let mut tracker = incremental_tracker();
        tracker.save_path = directory.0.to_str().unwrap().to_owned();
        tracker.sid = Some("sid123".to_owned());
        tracker.handler = handler;
        tracker.cleanup = cleanup;

        let chunks: &[(&[u8], &[(&str, usize, bool)])] = &[
            (concat!(
                "--BOUND\r\nContent-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\r\nkey\r\n",
                "--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"first\"\r\n\r\nfirst",
            ).as_bytes(), &[("first", 5, false)]),
            (b"-more", &[("first", 10, false)]),
            (b"\r\n--BOUND\r\nContent-Disposition: form-data; name=\"f\"; filename=\"second\"\r\n\r\nsecond",
                &[("first", 10, true), ("second", 6, false)]),
            (b"-payload", &[("first", 10, true), ("second", 14, false)]),
            (b"\r\n--BOUND--\r\n", &[("first", 10, true), ("second", 14, true)]),
        ];
        let poison = b"a:2:{s:15:\"bytes_processed\";i:-777;s:5:\"files\";a:0:{}}".to_vec();
        let mut body = Vec::new();
        for (step, (chunk, files)) in chunks.iter().enumerate() {
            // Allocate while the previous body is still alive, forcing relocation.
            let mut next = Vec::with_capacity(body.len() + chunk.len());
            next.extend_from_slice(&body);
            next.extend_from_slice(chunk);
            body = next;

            // Replace the old progress tree and vary an unrelated packed-array entry.
            let count = if step % 2 == 0 { 149 } else { 1 };
            let mut churn = format!("a:{count}:{{");
            for index in 0..count {
                churn.push_str(&format!("i:{index};a:0:{{}}"));
            }
            churn.push('}');
            let keep = format!("i:{step};").into_bytes();
            let mut pairs = vec![
                ("keep", keep),
                ("up_key", poison.clone()),
                ("churn", churn.into_bytes()),
            ];
            std::fs::write(&path, session_fixture(handler, &pairs)).unwrap();
            tracker.update(&body);
            assert_eq!(tracker.last_write_bytes, body.len(), "frame {step} must write");
            pairs[1].1 = progress_fixture(body.len(), false, files);
            assert_eq!(std::fs::read(&path).unwrap(), session_fixture(handler, &pairs));

            if step + 1 == chunks.len() {
                // Completion must also reload the latest session, including cleanup.
                pairs[0].1 = b"i:999;".to_vec();
                pairs[1].1 = poison.clone();
                std::fs::write(&path, session_fixture(handler, &pairs)).unwrap();
                tracker.complete(&body);
                if cleanup {
                    pairs.remove(1);
                } else {
                    pairs[1].1 = progress_fixture(body.len(), true, files);
                }
                assert_eq!(std::fs::read(&path).unwrap(), session_fixture(handler, &pairs));
            }
        }
    }
}

/// Repeated writes and file switches must keep counters independent of reloaded sessions.
#[test]
fn progress_survives_session_reloads_and_body_reallocation() {
    exercise_session_reloads(false);
}

/// Final cleanup must remove only progress after replacing the on-disk session again.
#[test]
fn cleanup_after_session_reloads_preserves_unrelated_entries() {
    exercise_session_reloads(true);
}
