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

#[test]
fn test_magic_constants_are_case_insensitive() {
    let out = compile_and_run("<?php echo __dir__ . '|'; echo __LiNe__;");
    assert!(out.starts_with('/'), "__dir__ should resolve to a path, got {:?}", out);
    assert!(out.ends_with("|1"), "__LiNe__ should resolve to line 1, got {:?}", out);
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
    assert!(
        out.contains("{closure:") && out.ends_with("test.php:2}"),
        "expected PHP-style closure marker, got {:?}",
        out
    );
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

#[test]
fn test_dunder_class_inside_trait_method_uses_importing_class() {
    let out = compile_and_run(
        "<?php\ntrait Greetable {\n    public function name() { echo __CLASS__ . '|' . __METHOD__; }\n}\nclass C { use Greetable; }\n$o = new C(); $o->name();\n",
    );
    assert_eq!(out, "C|Greetable::name");
}

#[test]
fn test_dunder_class_inside_trait_property_uses_importing_class() {
    let out = compile_and_run(
        "<?php\ntrait Named {\n    public string $name = __CLASS__;\n}\nclass C { use Named; }\n$o = new C(); echo $o->name;\n",
    );
    assert_eq!(out, "C");
}

#[test]
fn test_dunder_class_inside_namespaced_trait_uses_importing_class_fqn() {
    let out = compile_and_run(
        "<?php\nnamespace App;\ntrait Named {\n    public string $name = __CLASS__;\n    public function info() { echo __CLASS__ . '|' . __METHOD__ . '|' . __TRAIT__; }\n}\nclass C { use Named; }\n$o = new C(); echo $o->name . '|'; $o->info();\n",
    );
    assert_eq!(out, "App\\C|App\\C|App\\Named::info|App\\Named");
}

#[test]
fn test_dunder_function_inside_closure_in_method_uses_php_style_name() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public function m() {\n        $f = function() { echo __FUNCTION__ . '|' . __METHOD__; };\n        $f();\n    }\n}\n$o = new C(); $o->m();\n",
    );
    assert_eq!(out, "{closure:C::m():4}|{closure:C::m():4}");
}

#[test]
fn test_magic_constant_inside_short_ternary_is_lowered() {
    let out = compile_and_run(
        "<?php\nfunction f() {\n    echo __FUNCTION__ ?: 'fallback';\n    echo '|';\n    echo '' ?: __FUNCTION__;\n}\nf();\n",
    );
    assert_eq!(out, "f|f");
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

#[test]
fn test_included_file_magic_namespace_does_not_inherit_caller_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nrequire 'lib/inner.php';\necho '[' . __NAMESPACE__ . ']';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho '[' . __NAMESPACE__ . ']';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[][App]");
}

#[test]
fn test_included_file_namespace_does_not_leak_to_caller_magic_constants() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nrequire 'lib/inner.php';\necho '[' . __NAMESPACE__ . ']';\n",
            ),
            (
                "lib/inner.php",
                "<?php\nnamespace Lib;\necho '[' . __NAMESPACE__ . ']';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[Lib][App]");
}

#[test]
fn test_included_file_magic_function_does_not_inherit_calling_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfunction load() { require 'lib/inner.php'; }\nload();\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho '[' . __FUNCTION__ . ']';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[]");
}
