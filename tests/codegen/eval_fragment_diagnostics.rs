//! Purpose:
//! End-to-end tests for the location a diagnostic raised inside a literal `eval()` fragment
//! names: php's `FILE(LINE) : eval()'d code`, not the host file.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_fragment_diagnostics` through Rust's test harness.
//!
//! Key details:
//! - A literal fragment the compiler accepts is compiled ahead of time into ONE function shared
//!   by every `eval()` with the same text, so the fixtures run the same fragment from two lines
//!   (and from a function) to pin that each run names its own call site.
//! - Paths are cut with `basename()`: the test directory differs per run.
//! - Expected output is `php -n` 8.5's.

use crate::support::*;

/// Verifies an error handler's `$errfile`, and `error_get_last()['file']`, name the eval'd code
/// of the `eval()` call that raised the warning, line by line.
#[test]
fn test_warning_inside_a_compiled_eval_fragment_names_the_eval_site() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function ($no, $str, $file, $line) {
    echo "handler: $str @ ", basename($file), " line $line\n";
    return true;
});
function in_function() { eval("\n\$x = [];\necho \$x['infn'];\n"); }
eval("\n\$x = [];\necho \$x['missing'];\n");
in_function();
eval("\n\$x = [];\necho \$x['missing'];\n");
eval("\n\$u = [1];\necho \$u[5];\n");
restore_error_handler();
eval("\n\$y = [];\necho @\$y['nope'];\n");
$e = error_get_last();
echo basename($e['file']), " ", $e['line'], " ", $e['message'], "\n";
"#,
    );
    assert_eq!(
        out,
        "handler: Undefined array key \"missing\" @ test.php(7) : eval()'d code line 3\n\
         handler: Undefined array key \"infn\" @ test.php(6) : eval()'d code line 3\n\
         handler: Undefined array key \"missing\" @ test.php(9) : eval()'d code line 3\n\
         handler: Undefined array key 5 @ test.php(10) : eval()'d code line 3\n\
         test.php(12) : eval()'d code 3 Undefined array key \"nope\"\n"
    );
}
