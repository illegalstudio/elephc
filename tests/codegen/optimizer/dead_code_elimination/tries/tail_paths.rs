use super::*;

#[test]
fn test_dead_code_elimination_sinks_tail_into_try_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        echo "a";
    } catch (Exception $e) {
        return;
    }
    echo "b";
}

run(false);
run(true);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_try_catch_only_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        return;
    } catch (Exception $e) {
        echo "a";
    }
    echo "b";
}

run(true);
run(false);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}
