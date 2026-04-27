use crate::support::*;

// `require __DIR__ . '/...';` is the most common idiomatic include pattern
// in PHP. After magic-constant substitution, __DIR__ becomes a string literal
// and the resolver's path folder concatenates it with the trailing literal.

#[test]
fn test_include_with_dunder_dir_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

#[test]
fn test_include_with_const_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst LIB = 'lib';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

#[test]
fn test_include_with_define_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\ndefine('LIB', 'lib');\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

#[test]
fn test_include_with_nested_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/' . 'lib' . '/' . 'inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

#[test]
fn test_include_with_const_defined_in_included_file() {
    // The bootstrap file defines a constant; a subsequent require uses it.
    // The resolver tracks constants incrementally as it inlines files in
    // order, so this cross-file forward reference works.
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'bootstrap.php';\nrequire SUBDIR . '/inner.php';\necho 'after';\n",
            ),
            (
                "bootstrap.php",
                "<?php\nconst SUBDIR = 'lib';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

#[test]
fn test_include_with_dunder_file_dir_const() {
    // const can also be initialized from __DIR__/__FILE__ — substitute_file_constants
    // turns them into string literals before the resolver runs, so the
    // const-fold helper sees a plain BinaryOp(StringLiteral, Concat,
    // StringLiteral) and folds it.
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst BASE = __DIR__ . '/lib';\nrequire BASE . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}
