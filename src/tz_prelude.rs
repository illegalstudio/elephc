//! Purpose:
//! The timezone-introspection standard-library surface
//! (`timezone_location_get`/`timezone_transitions_get`/`timezone_abbreviations_list`
//! plus the marshalling helpers the `DateTimeZone` OOP methods delegate to),
//! written in elephc-PHP. Declares the `elephc_tz` bridge externs and parses
//! their serialized output into PHP arrays, so the feature compiles through the
//! normal pipeline (functions, extern C-ABI calls, arrays) with no new codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, after include/PDO
//!   injection and before name resolution.
//!
//! Key details:
//! - The prelude is injected only when a program references the introspection
//!   surface (see `detect`), so non-tz binaries never declare the `elephc_tz`
//!   externs and never link `libelephc_tz.a`. Its presence (the
//!   `__elephc_tz_location_get` marker) is what gates adding the three OOP methods
//!   to the synthetic `DateTimeZone` (see `inject_builtin_datetime`).
//! - `getTransitions($begin,$end)` is handled by one windowing routine whose
//!   defaults (`PHP_INT_MIN`/`PHP_INT_MAX`) reduce exactly to PHP's full no-arg
//!   list, reusing the bridge's row-0 `time` so `gmdate` is never asked to format
//!   `PHP_INT_MIN`.

use crate::parser::ast::Program;

mod detect;

/// The elephc-PHP timezone-introspection prelude: the `elephc_tz` extern block the
/// synthetic `DateTimeZone` methods call into, plus the three procedural aliases
/// that delegate to those methods. The array marshalling lives in the methods
/// (see `inject_builtin_datetime`), so it is written once; the procedural
/// functions are thin wrappers, matching PHP's procedural/OOP duality.
pub const TZ_PRELUDE_SRC: &str = r#"<?php

extern "elephc_tz" {
    function elephc_tz_location(string $zone): string;
    function elephc_tz_transitions(string $zone): string;
    function elephc_tz_abbreviations(): string;
}

function timezone_location_get(DateTimeZone $object) {
    return $object->getLocation();
}

function timezone_transitions_get(DateTimeZone $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX) {
    return $object->getTransitions($timestampBegin, $timestampEnd);
}

function timezone_abbreviations_list() {
    return DateTimeZone::listAbbreviations();
}
"#;

/// Prepends the timezone-introspection prelude to `program` when it references the
/// introspection surface, so the `elephc_tz` externs and helper functions compile
/// through the normal pipeline only for programs that use them. The prelude is
/// declarations only (extern block + functions), which are hoisted, so prepending
/// does not change top-level execution order. It is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than degrading.
pub fn inject_if_used(program: Program) -> Program {
    if !detect::program_uses_tz_introspection(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(TZ_PRELUDE_SRC).expect("tz prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("tz prelude must parse");
    combined.extend(program);
    combined
}
