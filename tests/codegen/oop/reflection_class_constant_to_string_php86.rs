//! Purpose:
//! PHP 8.6's `ReflectionClassConstant::__toString()` rendering, including boolean values.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_class_constant_to_string_php86`.
//!
//! Key details:
//! - php-src prints `Constant [ <final?><visibility> <type> <name> ] { <value> }`. Before 8.6
//!   the boolean branch reused the generic scalar text, so `true`/`false` were lost.
//! - The standalone constructor and the `ReflectionClass::getReflectionConstants()` entries
//!   share the same renderer.

use crate::support::*;

/// Boolean and scalar class constants render with php-src's dump format.
#[test]
fn test_reflection_class_constant_to_string_php86() {
    let out = compile_and_run(
        r#"<?php
class Demo {
    public const IS_TRUE = true;
    public const IS_FALSE = false;
    public const NUMBER = 42;
    protected final const NAME = 'demo';
}
echo new ReflectionClassConstant(Demo::class, 'IS_TRUE');
echo new ReflectionClassConstant(Demo::class, 'IS_FALSE');
echo new ReflectionClassConstant(Demo::class, 'NUMBER');
echo new ReflectionClassConstant(Demo::class, 'NAME');
foreach ((new ReflectionClass(Demo::class))->getReflectionConstants() as $constant) {
    echo $constant;
}
"#,
    );
    assert_eq!(
        out,
        "Constant [ public bool IS_TRUE ] { true }\n\
Constant [ public bool IS_FALSE ] { false }\n\
Constant [ public int NUMBER ] { 42 }\n\
Constant [ final protected string NAME ] { demo }\n\
Constant [ public bool IS_TRUE ] { true }\n\
Constant [ public bool IS_FALSE ] { false }\n\
Constant [ public int NUMBER ] { 42 }\n\
Constant [ final protected string NAME ] { demo }\n"
    );
}
