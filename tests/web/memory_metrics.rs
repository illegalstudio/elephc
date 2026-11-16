//! Purpose:
//! Verifies request-local memory peaks when one web worker reuses its PHP arena.
//!
//! Called from:
//! - The web integration test harness.
//!
//! Key details:
//! - Both requests use one worker, so process-wide peak retention is observable.

use super::*;

/// Owns one isolated fixture directory and removes it even if a request assertion fails.
struct FixtureDirectory(PathBuf);

impl Drop for FixtureDirectory {
    /// Removes only the directory created by this test after the server guard has stopped.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A small second request starts with a lower peak than the preceding large request.
#[test]
fn web_memory_metrics_peak_resets_between_requests() {
    let dir = FixtureDirectory(make_test_dir("web_memory_metrics"));
    let source = r#"<?php
if (isset($_GET["large"])) {
    $values = array_fill(0, 100000, 7);
    echo count($values), ":", memory_get_peak_usage();
} else {
    echo "0:", memory_get_peak_usage();
}
"#;
    let binary = compile_web(&dir.0, source, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = spawn_server(&binary, &addr, "1");
    let large = http_get(&addr, "/?large=1");
    let small = http_get(&addr, "/");
    let (count, large_peak) = raw_response_body(&large).trim().split_once(':').unwrap();
    assert_eq!(count, "100000", "{large}");
    let (count, small_peak) = raw_response_body(&small).trim().split_once(':').unwrap();
    assert_eq!(count, "0", "{small}");
    let large_peak: usize = large_peak.parse().unwrap();
    let small_peak: usize = small_peak.parse().unwrap();
    assert!(large_peak > 100000, "{large}");
    assert!(small_peak < large_peak, "large={large_peak}, next request={small_peak}");
}
