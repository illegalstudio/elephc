use crate::support::*;

#[test]
fn test_chained_variable_assign_value() {
    let out = compile_and_run(
        "<?php $a = $b = 7; echo $a + $b;",
    );
    assert_eq!(out, "14");
}

#[test]
fn test_chained_three_levels() {
    let out = compile_and_run(
        "<?php $a = $b = $c = 5; echo $a + $b + $c;",
    );
    assert_eq!(out, "15");
}

#[test]
fn test_chained_string_assignment() {
    let out = compile_and_run(
        "<?php $a = $b = \"hi\"; echo $a . $b;",
    );
    assert_eq!(out, "hihi");
}

#[test]
fn test_chained_static_prop_and_local() {
    // Mirrors the Composer pattern:
    //   self::$loader = $loader = expr;
    let out = compile_and_run(
        "<?php\nclass C {\n    public static int $x = 0;\n    public static function init(): int {\n        self::$x = $local = 42;\n        return self::$x + $local;\n    }\n}\necho C::init();\n",
    );
    assert_eq!(out, "84");
}
