use crate::support::*;

// `__FILE__` and `__DIR__` are substituted to the canonical path of the
// containing source file. The test harness uses `<temp>/test.php` as the
// synthetic main path, so __FILE__ ends with "test.php" and __DIR__ is the
// canonical temp directory.

#[test]
fn test_dunder_file_is_absolute_path_ending_in_test_php() {
    let out = compile_and_run("<?php echo __FILE__;");
    assert!(
        out.starts_with('/'),
        "__FILE__ should be absolute, got {:?}",
        out
    );
    assert!(
        out.ends_with("test.php"),
        "__FILE__ should end with test.php, got {:?}",
        out
    );
}

#[test]
fn test_dunder_dir_is_absolute_path_with_no_trailing_slash() {
    let out = compile_and_run("<?php echo __DIR__;");
    assert!(out.starts_with('/'), "__DIR__ should be absolute, got {:?}", out);
    assert!(
        !out.ends_with('/'),
        "__DIR__ should not end with a separator, got {:?}",
        out
    );
}

#[test]
fn test_dunder_dir_concat_produces_single_folded_string() {
    // The optimizer should fold `__DIR__ . '/sub'` into a single literal.
    let out = compile_and_run("<?php echo __DIR__ . '/sub/file.php';");
    assert!(out.starts_with('/'));
    assert!(out.ends_with("/sub/file.php"));
}

// `__LINE__` is substituted at parse time using the span line.

#[test]
fn test_dunder_line_at_first_line() {
    let out = compile_and_run("<?php echo __LINE__;");
    assert_eq!(out, "1");
}

#[test]
fn test_dunder_line_after_blank_lines() {
    let out = compile_and_run("<?php\n\n\necho __LINE__;\n");
    assert_eq!(out, "4");
}

#[test]
fn test_dunder_line_inside_function_body() {
    let out = compile_and_run(
        "<?php\nfunction f() {\n    echo __LINE__;\n}\nf();\n",
    );
    assert_eq!(out, "3");
}

// `__FUNCTION__` returns the (FQN) function name inside a function, empty
// outside.

#[test]
fn test_dunder_function_outside_any_function_is_empty() {
    let out = compile_and_run("<?php echo '[' . __FUNCTION__ . ']';");
    assert_eq!(out, "[]");
}

#[test]
fn test_dunder_function_inside_plain_function() {
    let out = compile_and_run(
        "<?php\nfunction greet() {\n    echo __FUNCTION__;\n}\ngreet();\n",
    );
    assert_eq!(out, "greet");
}

#[test]
fn test_dunder_function_inside_namespaced_function_uses_fqn() {
    let out = compile_and_run(
        "<?php\nnamespace App\\Util;\nfunction greet() {\n    echo __FUNCTION__;\n}\ngreet();\n",
    );
    assert_eq!(out, "App\\Util\\greet");
}

#[test]
fn test_dunder_function_inside_closure_returns_closure_marker() {
    let out = compile_and_run(
        "<?php\n$f = function() {\n    echo __FUNCTION__;\n};\n$f();\n",
    );
    assert_eq!(out, "{closure}");
}

// `__CLASS__` returns the (FQN) class name, empty outside.

#[test]
fn test_dunder_class_outside_any_class_is_empty() {
    let out = compile_and_run("<?php echo '[' . __CLASS__ . ']';");
    assert_eq!(out, "[]");
}

#[test]
fn test_dunder_class_inside_method() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public function m() { echo __CLASS__; }\n}\n$o = new C(); $o->m();\n",
    );
    assert_eq!(out, "C");
}

#[test]
fn test_dunder_class_inside_namespaced_class_uses_fqn() {
    let out = compile_and_run(
        "<?php\nnamespace App;\nclass C {\n    public function m() { echo __CLASS__; }\n}\n$o = new C(); $o->m();\n",
    );
    assert_eq!(out, "App\\C");
}

// `__METHOD__` returns "Class::method" inside a method.

#[test]
fn test_dunder_method_outside_any_function_is_empty() {
    let out = compile_and_run("<?php echo '[' . __METHOD__ . ']';");
    assert_eq!(out, "[]");
}

#[test]
fn test_dunder_method_inside_method_is_class_qualified() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public function go() { echo __METHOD__; }\n}\n$o = new C(); $o->go();\n",
    );
    assert_eq!(out, "C::go");
}

#[test]
fn test_dunder_method_inside_namespaced_method_uses_fqn() {
    let out = compile_and_run(
        "<?php\nnamespace App;\nclass C {\n    public function go() { echo __METHOD__; }\n}\n$o = new C(); $o->go();\n",
    );
    assert_eq!(out, "App\\C::go");
}

#[test]
fn test_dunder_method_inside_plain_function_is_function_name() {
    let out = compile_and_run(
        "<?php\nfunction f() { echo __METHOD__; }\nf();\n",
    );
    assert_eq!(out, "f");
}

// `__NAMESPACE__` returns the current namespace, empty outside.

#[test]
fn test_dunder_namespace_outside_namespace_is_empty() {
    let out = compile_and_run("<?php echo '[' . __NAMESPACE__ . ']';");
    assert_eq!(out, "[]");
}

#[test]
fn test_dunder_namespace_inside_namespace_decl() {
    let out = compile_and_run(
        "<?php\nnamespace App\\Util;\necho __NAMESPACE__;\n",
    );
    assert_eq!(out, "App\\Util");
}

// `__TRAIT__` returns the trait name inside a trait method.

#[test]
fn test_dunder_trait_outside_trait_is_empty() {
    let out = compile_and_run("<?php echo '[' . __TRAIT__ . ']';");
    assert_eq!(out, "[]");
}

#[test]
fn test_dunder_trait_inside_trait_method() {
    let out = compile_and_run(
        "<?php\ntrait Greetable {\n    public function name() { echo __TRAIT__; }\n}\nclass C { use Greetable; }\n$o = new C(); $o->name();\n",
    );
    assert_eq!(out, "Greetable");
}

// `__FILE__` and `__DIR__` from an *included* file should reflect that
// file's path, not the main file's.

#[test]
fn test_dunder_file_inside_include_uses_included_files_path() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho __FILE__;\n",
            ),
        ],
        "main.php",
    );
    assert!(
        out.ends_with("/lib/inner.php"),
        "expected included file's __FILE__, got {:?}",
        out
    );
}

#[test]
fn test_dunder_dir_inside_include_uses_included_files_dir() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho __DIR__;\n",
            ),
        ],
        "main.php",
    );
    assert!(
        out.ends_with("/lib"),
        "expected included file's __DIR__ (ending in /lib), got {:?}",
        out
    );
}
