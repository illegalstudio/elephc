//! The CLI's end-of-build teardown.

use crate::support::*;

/// A CLI compile forgets the codegen state and the data section instead of dropping them: the
/// process exits right after, and tearing them down cost about 7 s of the Symfony demo's
/// single-core build. The program still compiles and runs as php does.
#[test]
fn test_a_cli_build_forgets_its_codegen_state_instead_of_dropping_it() {
    let main = "<?php\nclass Point { public function __construct(public int $x) {} }\n$p = new Point(4);\necho $p->x * 2, \"\\n\";\n";
    let (stderr, out, _) = compile_cli_files_with_env_and_run(
        &[("main.php", main)],
        "main.php",
        &[("ELEPHC_TEARDOWN_TRACE", "1")],
    );
    assert_eq!(out, "8\n");
    let forgotten = stderr
        .lines()
        .find_map(|line| line.strip_prefix("[elephc-teardown] forgotten="))
        .and_then(|count| count.trim().parse::<usize>().ok())
        .unwrap_or_else(|| panic!("no teardown trace:\n{stderr}"));
    assert!(forgotten >= 2, "the codegen state was dropped:\n{stderr}");
}
