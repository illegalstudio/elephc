//! Purpose:
//! Integration tests for the LINE COUNT an `SplFileObject` iteration yields and what `eof()`
//! reports at each step of it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php drives the iteration from the stream and reads one line AHEAD of the one it yields, so
//!   `eof()` is already true while the LAST element is still current. elephc iterates a line
//!   ARRAY, whose cursor cannot see the stream, and answered `false` at every step.
//! - The read-ahead is also what decides whether there IS a trailing empty element: after the
//!   final `\n` a plain file is not yet at end of file, so one more round answers `''`. A
//!   `php://temp` stream IS at end of file there — it reports EOF as soon as a line read drains
//!   it — so an `SplTempFileObject` over the same bytes yields one element FEWER.
//! - MEASURED on `php -n` 8.5.6 for four bodies × both kinds. `"a\n"` is the sharpest: the plain
//!   file yields two elements and the temp object one.
//! - Reading by hand is the other half of the rule: `fgets()` does not read ahead, so it takes
//!   the object back onto the stream's own answer.

use crate::support::*;

/// Verifies the element count and `eof()` at every step, for both kinds of backing.
#[test]
fn the_iteration_reads_one_line_ahead_of_what_it_yields() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function walk(string $label, SplFileObject $o): void {
    echo $label, ": start=", var_export($o->eof(), true), " ";
    $o->rewind();
    while ($o->valid()) {
        echo "[k=", $o->key(), " len=", strlen((string)$o->current()),
             " eof=", var_export($o->eof(), true), "] ";
        $o->next();
    }
    echo "end=", var_export($o->eof(), true), " key=", $o->key(), "\n";
}
foreach (["a\nb\n", "a\nb", "a\n", ""] as $i => $body) {
    $t = new SplTempFileObject();
    if ($body !== "") { $t->fwrite($body); }
    walk("temp $i", $t);
    file_put_contents("v.txt", $body);
    walk("plain $i", new SplFileObject("v.txt"));
}
"#,
    );
    assert_eq!(
        out,
        "temp 0: start=false [k=0 len=2 eof=false] [k=1 len=2 eof=true] end=true key=2\n\
         plain 0: start=false [k=0 len=2 eof=false] [k=1 len=2 eof=false] [k=2 len=0 eof=true] end=true key=3\n\
         temp 1: start=false [k=0 len=2 eof=false] [k=1 len=1 eof=true] end=true key=2\n\
         plain 1: start=false [k=0 len=2 eof=false] [k=1 len=1 eof=true] end=true key=2\n\
         temp 2: start=false [k=0 len=2 eof=true] end=true key=1\n\
         plain 2: start=false [k=0 len=2 eof=false] [k=1 len=0 eof=true] end=true key=2\n\
         temp 3: start=false [k=0 len=0 eof=true] end=true key=1\n\
         plain 3: start=false [k=0 len=0 eof=true] end=true key=1\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies a HAND-DRIVEN read takes the object back onto the stream's own answer.
///
/// Without that, the `rewind()` above still spoke for a cursor the caller had stopped using, and
/// the first `fgets()` of a temp object reported the end of a stream with half its bytes unread.
#[test]
fn reading_by_hand_does_not_read_ahead() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$t = new SplTempFileObject();
$t->fwrite("a\nb\n");
$t->rewind();
echo "temp ", strlen((string)$t->fgets()), "=", var_export($t->eof(), true);
echo " ", strlen((string)$t->fgets()), "=", var_export($t->eof(), true), "\n";
file_put_contents("h.txt", "a\nb\n");
$f = new SplFileObject("h.txt");
echo "plain ", strlen((string)$f->fgets()), "=", var_export($f->eof(), true);
echo " ", strlen((string)$f->fgets()), "=", var_export($f->eof(), true), "\n";
"#,
    );
    assert_eq!(out, "temp 2=false 2=true\nplain 2=false 2=false\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `current()` works on an object nothing has rewound.
///
/// `seekState` was READ by `current()` and `valid()` but only WRITTEN by `rewind()` and `seek()`,
/// so php's own first-line idiom died on an uninitialized typed property.
#[test]
fn current_answers_the_first_line_with_no_rewind() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("w.txt", "x\ny\n");
$g = new SplFileObject("w.txt");
echo var_export($g->current(), true), " key=", $g->key(), "\n";
"#,
    );
    assert_eq!(out, "'x\n' key=0\n");
    let _ = std::fs::remove_dir_all(dir);
}
