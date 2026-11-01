//! Purpose:
//! Interpreter tests for the source line reported from inside an INCLUDED file, which stayed at
//! the line the include began on for every statement of the file.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::include_line_tracking`.
//!
//! Key details:
//! - `__LINE__` was never the problem: the lexer stamps it into the token, and `parse_source_file`
//!   keeps file-relative lines, so it was already right. What was wrong is everything that reads
//!   the CONTEXT's position -- `debug_backtrace()` frames, a diagnostic's `on line N`, and the
//!   `FILE(LINE) : eval()'d code` spelling -- because nothing moved that position per statement.
//! - `EvalStmt::SourceLine` markers, emitted only for a whole-FILE parse, move it. They cost the
//!   fragment path nothing, which is why the parser's 262 fragment shape expectations are
//!   untouched and only the four `parse_source_file` ones changed.
//! - Every expected number is `php -n` 8.5.6's output on the same two files.

use super::super::*;
use super::support::*;

/// Runs a caller fragment against files on disk and returns what it echoed.
fn run_include_fixture(tag: &str, files: &[(&str, &str)], fragment: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc-magician-include-lines-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include line fixture directory");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write include line fixture");
    }
    let program = parse_fragment(fragment).expect("parse include line caller fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let outcome = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
    let output = values.output.clone();
    let _ = std::fs::remove_dir_all(&dir);
    outcome.expect("execute include line caller fragment");
    output
}

/// One included file exercising every position an interpreted include reports.
///
/// Line numbers matter here, so they are listed rather than left to be counted:
/// 2 and 3 are the two `__LINE__` echoes, 5 is `inner()`'s own `__LINE__`, 11 is the call to
/// `inner()` inside `outer()`, 13 is the call to `outer()`, 15 is the closure body, and 19 is the
/// `eval()`.
const POSITIONS: &str = r#"<?php
echo "L:", __LINE__, ";";
echo "L:", __LINE__, ";";
function inner() {
    echo "inL:", __LINE__, ";";
    $bt = debug_backtrace();
    echo "bt0:", $bt[0]['line'], ";";
    echo "bt1:", isset($bt[1]) ? $bt[1]['line'] : "none", ";";
}
function outer() {
    inner();
}
outer();
$f = function () {
    echo "clL:", __LINE__, ";";
};
$f();
echo "e:";
eval('echo basename(__FILE__), ";";');
echo "after;";
"#;

/// Verifies every position an included file reports is the included file's own line.
///
/// `php -n` 8.5.6 prints `L:2;L:3;inL:5;bt0:11;bt1:13;clL:15;e:inc.php(19) : eval()'d code;after;`.
///
/// The two backtrace lines are the measurement. `bt0` is the line the CALL to `inner()` was
/// written on -- 11, inside `outer()` -- and `bt1` is the line `outer()` was called from, 13.
/// Both read the context's position, and both were 1 before, because entering the file set the
/// position once and nothing moved it again. `inL` and `clL` are asserted beside them precisely
/// because they were ALREADY right: `__LINE__` comes from the lexer, so a fix that only made
/// `__LINE__` work would have changed nothing at all.
///
/// The `eval()` spelling is the third reader: php says `FILE(LINE) : eval()'d code`, and both
/// halves were wrong -- the line for the same reason as the backtrace, and the FILE because an
/// include leaves a `__FILE__` override in place that a nested `eval()` has to step out of.
#[test]
fn every_position_inside_an_included_file_is_that_files_own_line() {
    let output = run_include_fixture(
        "positions",
        &[("inc.php", POSITIONS)],
        br#"include "inc.php";"#,
    );
    let output = output.replace(&format!("{}/", std::env::temp_dir().display()), "");
    let cleaned = output
        .split(';')
        .map(|piece| match piece.rfind('/') {
            Some(index) => &piece[index + 1..],
            None => piece,
        })
        .collect::<Vec<_>>()
        .join(";");
    assert_eq!(
        cleaned,
        "L:2;L:3;inL:5;bt0:11;bt1:13;clL:15;e:inc.php(19) : eval()'d code;after;",
    );
}

/// Verifies a frame names the file the CALL was written in, and its line THERE.
///
/// `php -n` 8.5.6 prints `f0:caller.php;l0:5;`. Two included files: `lib.php` declares `probe()`,
/// `caller.php` calls it on its own line 5. The frame must name `caller.php` and 5 -- not
/// `lib.php`, where the body runs, and not the entry file, where the includes were written.
///
/// This is the shape a single moving position still cannot fake: it needs the position to follow
/// the statement WHILE `caller.php` executes, and to be captured at the moment the call is made
/// rather than read back after the body has moved it again.
#[test]
fn a_frame_names_the_file_the_call_was_written_in_and_its_line_there() {
    let output = run_include_fixture(
        "two_files",
        &[
            (
                "lib.php",
                "<?php\nfunction probe() {\n    $bt = debug_backtrace();\n    echo \"f0:\", basename($bt[0][\"file\"]), \";l0:\", $bt[0][\"line\"], \";\";\n}\n",
            ),
            (
                "caller.php",
                "<?php\n$pad = 1;\n$pad = 2;\n$pad = 3;\nprobe();\n",
            ),
        ],
        br#"include "lib.php";
include "caller.php";"#,
    );
    assert_eq!(output, "f0:caller.php;l0:5;");
}

/// Runs a caller fragment with the include target slot armed, and returns what it recorded.
///
/// Armed for THIS THREAD only. Switching the whole trace on would be process-wide -- through the
/// environment, which other test threads are reading, or through the cached verdict, which every
/// other trace site consults -- and one of those sites dereferences a result cell without a null
/// check, so arming the process aborts the binary from an unrelated test.
fn run_traced_include_fixture(
    tag: &str,
    files: &[(&str, &str)],
    fragment: &[u8],
) -> (
    std::path::PathBuf,
    Option<std::path::PathBuf>,
    Option<std::path::PathBuf>,
) {
    let dir = std::env::temp_dir().join(format!(
        "elephc-magician-include-target-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include target fixture directory");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write include target fixture");
    }
    let program = parse_fragment(fragment).expect("parse include target caller fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    crate::eval_trace::arm_include_target_for_test(true);
    let _ = crate::eval_trace::take_include_target();
    let outcome = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
    let recorded = crate::eval_trace::take_include_target();
    // Taken a second time while the slot is still armed, because after the disarm below it answers
    // `None` for the trivial reason that nothing is recording.
    let after_take = crate::eval_trace::take_include_target();
    crate::eval_trace::arm_include_target_for_test(false);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.expect("execute include target caller fragment");
    (dir, recorded, after_take)
}

/// Verifies an include hands the trace the RESOLVED file it reached, not the text that was written.
///
/// `phase=include_ok` is printed by the bridge entry point after the include returns, and the only
/// thing it knows by itself is the caller's position -- which after include splicing names a file
/// that does not contain the line it prints beside it. Naming the target is therefore the one
/// field a per-request include census can read, and this pins all three ways it could be useless:
/// missing, still relative, or -- the subtle one -- the target of a NESTED include rather than the
/// one that just finished. `outer.php` includes `inner.php`, so recording at resolution time
/// instead of on the way out would answer `inner.php` here.
#[test]
fn an_include_records_the_resolved_target_of_the_include_that_finished() {
    let (dir, recorded, after_take) = run_traced_include_fixture(
        "nested",
        &[
            ("inner.php", "<?php\necho \"in;\";\n"),
            ("outer.php", "<?php\ninclude \"inner.php\";\necho \"out;\";\n"),
        ],
        br#"include "outer.php";"#,
    );
    assert_eq!(
        recorded.as_deref(),
        Some(dir.join("outer.php").as_path()),
        "the trace must name the resolved target of the include that finished",
    );
    assert_eq!(
        after_take, None,
        "taking the target must empty the slot, so a later include cannot print a stale one",
    );
}

/// Verifies the target is recorded even when the file does not exist, so a miss is still named.
///
/// An `include` of a missing file warns and returns false; without this the census would show a
/// crossing whose target field said nothing at all, which is the exact failure the field exists
/// to end.
#[test]
fn a_missing_include_still_records_the_path_it_tried() {
    let (_dir, recorded, _after_take) =
        run_traced_include_fixture("missing", &[], br#"include "absent_probe.php";"#);
    assert_eq!(
        recorded.as_deref(),
        Some(std::path::Path::new("absent_probe.php")),
        "an unresolvable include must still report the path it tried",
    );
}
