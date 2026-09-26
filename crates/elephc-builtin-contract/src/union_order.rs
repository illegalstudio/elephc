//! Purpose:
//! The order PHP prints a union's members in, shared by the compiler and the interpreter.
//!
//! Called from:
//! - `src/codegen/lower_inst/objects/reflection` for compiled reflection.
//! - `crates/elephc-magician/src/interpreter/reflection` for reflection inside `eval()`.
//!
//! Key details:
//! - PHP renders a union from its internal type mask, which has a fixed order, so `int|string`
//!   prints as `string|int`. Both surfaces must agree: the point of issue #1118 was that the
//!   same program answered differently depending on which one was asked.

/// Ranks one union member name the way PHP orders them when printing a union.
///
/// Derived by measurement against PHP 8.5.10:
///
/// | declared | printed |
/// | --- | --- |
/// | `int\|string` | `string\|int` |
/// | `bool\|array\|string` | `array\|string\|bool` |
/// | `C\|int\|string` | `C\|string\|int` |
/// | `int\|C\|I` | `C\|I\|int` |
/// | `false\|int` | `int\|false` |
/// | `array\|object\|string` | `object\|array\|string` |
/// | `C\|float\|int` | `C\|int\|float` |
///
/// Class names come first and keep THEIR declared order among themselves, which a stable sort on
/// this rank preserves because they all share rank 0.
pub fn union_member_rank(name: &str) -> u8 {
    match name {
        "static" => 1,
        "callable" => 2,
        "object" => 3,
        "array" => 4,
        "iterable" => 5,
        "string" => 6,
        "int" => 7,
        "float" => 8,
        "bool" => 9,
        "false" => 10,
        "true" => 11,
        "null" => 12,
        // A class, interface or enum name, or a type this table does not know: PHP prints those
        // ahead of the built-ins, in the order they were written.
        _ => 0,
    }
}
