//! Purpose:
//! Exercises native string scratch lifetimes across synchronous host callback reentry.
//!
//! Called from:
//! - The real PHP SDK integration test binary in `tests/phpext_tests.rs`.
//!
//! Key details:
//! - A live concat result crosses the former 64 KiB boundary before PHP reenters.

use super::*;

/// Nested exports must not overwrite an outer concat temporary while its output callback runs.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_reentry_preserves_large_concat_temporaries() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("large_reentry", r#"<?php
#[Export] function php_large_outer(string $value): string {
    return ($value . 'TAIL') . (print 'chunk');
}
#[Export] function php_large_inner(string $value): string { return $value . 'nested'; }
"#);
    let driver = r#"
$input = str_repeat('a', 70000);
ob_start(function ($text) {
    if ($text !== '') {
        $nested = str_repeat('b', 8000);
        if (php_large_inner($nested) !== $nested . 'nested') { exit(10); }
    }
    return '';
}, 1);
$result = php_large_outer($input);
ob_end_clean();
if ($result !== $input . 'TAIL1') { fwrite(STDERR, 'outer concat corrupted'); exit(11); }
echo 'ok';
"#;
    let result = php(&sdk, &module).args(["-r", driver]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok");
    assert!(result.stderr.is_empty());
}

/// Successful reentry cannot erase an outer host exception while committing its own state.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn successful_reentry_preserves_pending_host_status() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("status_reentry", r#"<?php
#[Export] function php_status_outer(array $value): void {
    try { echo $value['missing']; } finally { echo 'cleanup'; }
}
#[Export] function php_status_tick(): int {
    static $state = 0;
    return ++$state;
}
"#);
    let result = php(&sdk, &module).args(["-d", "display_errors=0", "-r", r#"
$nested = [];
ob_start(function ($bytes) use (&$nested) {
    if ($bytes !== '') { $nested[] = php_status_tick(); }
    return '';
}, 1);
set_error_handler(function () { throw new LogicException('host'); });
try { php_status_outer([]); $caught = 'missing'; }
catch (Throwable $error) { $caught = get_class($error) . ':' . $error->getMessage(); }
restore_error_handler();
ob_end_clean();
echo json_encode([$caught, $nested, php_status_tick()]);
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, br#"["LogicException:host",[1],2]"#);
    assert!(result.stderr.is_empty(), "{}", String::from_utf8_lossy(&result.stderr));
}
