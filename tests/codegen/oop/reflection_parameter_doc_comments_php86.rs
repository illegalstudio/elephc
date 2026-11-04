//! Purpose:
//! Runs the PHP 8.6 ReflectionParameter::getDocComment() fixtures and normalizes EXPECTF widths.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_parameter_doc_comments_php86`.
//!
//! Key details:
//! - Sources are transcribed from `ext/reflection/tests/ReflectionParameter_getDocComment_{basic,indented}.phpt`.
//! - The basic fixture drops the closure/arrow cases because `new ReflectionFunction($closure)` is not
//!   supported yet; their parser channel is exercised by `tests/parser_tests/functions.rs`.
//! - `%d` in the upstream EXPECTF sections is normalized before comparison.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compares output using php-src's `EXPECTF` `%d` normalization.
fn assert_doc_output(actual: &str, expected: &str) {
    let normalize = |text: &str| {
        let mut normalized = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("string(") {
            normalized.push_str(&rest[..start]);
            normalized.push_str("string(%d)");
            rest = &rest[start + "string(".len()..];
            match rest.find(')') {
                Some(end) => rest = &rest[end + 1..],
                None => break,
            }
        }
        normalized.push_str(rest);
        normalized.trim().to_string()
    };
    assert_eq!(normalize(actual), normalize(expected));
}

/// php-src `ReflectionParameter_getDocComment_basic.phpt` (function/method/hook cases).
#[test]
fn test_reflection_parameter_doc_comment_basic_php86() {
    let out = compile_and_run_with_php_version(
        r#"<?php

class A
{
function method(
    /**
     * My Doc Comment for $a
     *
     */
    $a, $b, $c,
    /**
     * My Doc Comment for $d
     */
    $d,
    // Not a doc comment
    /**Not a doc comment */
    $e,
    /**
     * Doc comment for $f
     */
    $f,
	$g /** Doc comment for $g after parameter */,
	/** Doc comment for $h */
	$h /** Doc comment for $h after parameter */,
) {}

public string $property {
     set(
          /** Doc Comment for property hook parameter $value */
          string $value
     ) { $this->property = $value; }
}
}

function global_function(
    /**
     * My Doc Comment for $a
     *
     */
    $a, $b, $c,
    /**
     * My Doc Comment for $d
     */
    $d,
    // Not a doc comment
    /**Not a doc comment */
    $e,
    /**
     * Doc comment for $f
     */
    $f,
	$g /** Doc comment for $g after parameter */,
	/** Doc comment for $h */
	$h /** Doc comment for $h after parameter */,
) {}

foreach([
		'A::method'       => (new ReflectionClass('A'))->getMethod('method'),
		'global_function' => new ReflectionFunction('global_function'),
		'property hook'   => (new ReflectionClass('A'))->getProperty('property')->getHook(PropertyHookType::Set),
	] as $function => $rc) {
    $rps = $rc->getParameters();
    foreach($rps as $rp) {
        echo "\n---> Doc comment for $function parameter $" . $rp->getName() . ":\n";
        var_dump($rp->getDocComment());
    }
}

"#,
        PhpVersion::Php86,
    );
    assert_doc_output(&out, r#"---> Doc comment for A::method parameter $a:
string(%d) "/**
     * My Doc Comment for $a
     *
     */"

---> Doc comment for A::method parameter $b:
bool(false)

---> Doc comment for A::method parameter $c:
bool(false)

---> Doc comment for A::method parameter $d:
string(%d) "/**
     * My Doc Comment for $d
     */"

---> Doc comment for A::method parameter $e:
bool(false)

---> Doc comment for A::method parameter $f:
string(%d) "/**
     * Doc comment for $f
     */"

---> Doc comment for A::method parameter $g:
string(%d) "/** Doc comment for $g after parameter */"

---> Doc comment for A::method parameter $h:
string(%d) "/** Doc comment for $h after parameter */"

---> Doc comment for global_function parameter $a:
string(%d) "/**
     * My Doc Comment for $a
     *
     */"

---> Doc comment for global_function parameter $b:
bool(false)

---> Doc comment for global_function parameter $c:
bool(false)

---> Doc comment for global_function parameter $d:
string(%d) "/**
     * My Doc Comment for $d
     */"

---> Doc comment for global_function parameter $e:
bool(false)

---> Doc comment for global_function parameter $f:
string(%d) "/**
     * Doc comment for $f
     */"

---> Doc comment for global_function parameter $g:
string(%d) "/** Doc comment for $g after parameter */"

---> Doc comment for global_function parameter $h:
string(%d) "/** Doc comment for $h after parameter */"

---> Doc comment for property hook parameter $value:
string(%d) "/** Doc Comment for property hook parameter $value */"
"#);
}

/// php-src `ReflectionParameter_getDocComment_indented.phpt`.
#[test]
fn test_reflection_parameter_doc_comment_indented_php86() {
    let out = compile_and_run_with_php_version(
        r#"<?php

class A {
     function method(
     /**
          * My Doc Comment for $a
          *
          */
     $a, $b, $c,
     /**
          * My Doc Comment for $d
          */
     $d,
     // Not a doc comment
     /**Not a doc comment */
     $e,
     /**
          * Doc comment for $f
          */
     $f,
          $g /** Doc comment for $g after parameter */,
          /** Doc comment for $h */
          $h /** Doc comment for $h after parameter */,
     ) {}

     public string $property {
          set(
               /** Doc Comment for property hook parameter $value */
               string $value
          ) { $this->property = $value; }
     }
}

foreach([
        'A::method'       => (new ReflectionClass('A'))->getMethod('method'),
        'property hook'   => (new ReflectionClass('A'))->getProperty('property')->getHook(PropertyHookType::Set),
    ] as $function => $rc) {
    $rps = $rc->getParameters();
    foreach($rps as $rp) {
        echo "\n---> Doc comment for $function parameter $" . $rp->getName() . ":\n";
        var_dump($rp->getDocComment());
    }
}

"#,
        PhpVersion::Php86,
    );
    assert_doc_output(&out, r#"---> Doc comment for A::method parameter $a:
string(%d) "/**
          * My Doc Comment for $a
          *
          */"

---> Doc comment for A::method parameter $b:
bool(false)

---> Doc comment for A::method parameter $c:
bool(false)

---> Doc comment for A::method parameter $d:
string(%d) "/**
          * My Doc Comment for $d
          */"

---> Doc comment for A::method parameter $e:
bool(false)

---> Doc comment for A::method parameter $f:
string(%d) "/**
          * Doc comment for $f
          */"

---> Doc comment for A::method parameter $g:
string(%d) "/** Doc comment for $g after parameter */"

---> Doc comment for A::method parameter $h:
string(%d) "/** Doc comment for $h after parameter */"

---> Doc comment for property hook parameter $value:
string(%d) "/** Doc Comment for property hook parameter $value */"
"#);
}
