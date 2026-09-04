//! Purpose:
//! Integration tests for `SplFileObject`'s constructor: what it does when it cannot open the
//! file, and where it reads its lines from.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `new SplFileObject($p)` is php's way of saying "open this or fail loudly". elephc failed
//!   QUIETLY: MEASURED, it printed THREE warnings php never prints — `fopen()`, then `file()` on
//!   the same missing path, then `foreach` over the `false` that came back — and handed the
//!   program a live object whose stream was `false`.
//! - A DIRECTORY is refused before anything is opened, and as a `LogicException`, not a
//!   `RuntimeException`: php will not let an `SplFileObject` wrap one at all.
//! - THE LINES COME FROM THE STREAM, not from the path. `file($this->backingPath)` re-opened the
//!   file by NAME, and a stream with no name to re-open — `php://memory`, `php://temp` — came
//!   back empty every time.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies that a constructor which cannot open its file throws what php throws.
///
/// The three cases are three different reasons, and php words two of them the same way while
/// giving the directory its own class — which is what a single `RuntimeException` for everything
/// would have got wrong.
#[test]
fn a_file_object_that_cannot_open_throws_the_way_php_does() {
    let out = compile_and_run_capture(
        r#"<?php
try { new SplFileObject("nope.txt"); echo "missing no throw\n"; }
catch (Throwable $e) { echo "missing ", get_class($e), ": ", $e->getMessage(), "\n"; }

try { new SplFileObject("."); echo "dir no throw\n"; }
catch (Throwable $e) { echo "dir ", get_class($e), ": ", $e->getMessage(), "\n"; }

try { new SplFileObject("no/such/dir/f.txt", "w"); echo "write-nodir no throw\n"; }
catch (Throwable $e) { echo "write-nodir ", get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "missing RuntimeException: SplFileObject::__construct(nope.txt): Failed to open stream: No such file or directory\n\
         dir LogicException: Cannot use SplFileObject with directories\n\
         write-nodir RuntimeException: SplFileObject::__construct(no/such/dir/f.txt): Failed to open stream: No such file or directory\n"
    );
    assert_eq!(
        out.diagnostics, "",
        "php's constructor opens the stream itself and throws; it prints no warnings"
    );
}

/// Verifies a file object still opens, reads and writes an ordinary file.
///
/// The guard above sits in front of every construction, so this is what says it lets the
/// ordinary case through — including a write mode on a file that does not exist yet.
#[test]
fn an_ordinary_file_still_opens_reads_and_writes() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("ok.txt", "a\nb\n");
$o = new SplFileObject("ok.txt");
foreach ($o as $i => $line) { echo $i, ":", rtrim($line, "\n"), ";"; }
echo "\n";
$w = new SplFileObject("made.txt", "w");
$w->fwrite("written\n");
unset($w);
echo "wrote ", var_export(file_get_contents("made.txt"), true), "\n";
$info = new SplFileInfo("ok.txt");
echo "openFile ", $info->openFile()->getFilename(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0:a;1:b;2:;\n\
         wrote 'written\n'\n\
         openFile ok.txt\n"
    );
}

/// Verifies the lines come from the STREAM, for a stream that has no name to re-open.
///
/// `php://memory` is the case that proves it: re-opening that URL yields a NEW empty stream, so a
/// path-based reload answered an empty file no matter what had been written.
#[test]
fn the_lines_come_from_the_stream_not_from_the_path() {
    let out = compile_and_run_capture(
        r#"<?php
$m = new SplFileObject("php://memory", "r+");
$m->fwrite("mem\none\n");
$m->rewind();
foreach ($m as $i => $line) { echo $i, ":", var_export($line, true), ";"; }
echo "\n";
file_put_contents("rw.txt", "first\n");
$f = new SplFileObject("rw.txt", "r+");
$f->fseek(0, SEEK_END);
$f->fwrite("second\n");
$f->rewind();
foreach ($f as $i => $line) { echo $i, ":", rtrim($line, "\n"), ";"; }
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0:'mem\n';1:'one\n';2:'';\n\
         0:first;1:second;2:;\n"
    );
}
