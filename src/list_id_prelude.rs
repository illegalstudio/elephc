//! Purpose:
//! Injects the `__elephc_list_identifiers` standard-library function (written in
//! elephc-PHP) that backs `DateTimeZone::listIdentifiers($group, $country)` and
//! its `timezone_identifiers_list()` alias with real group/country filtering.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness, after include/PDO/
//!   tz injection and before name resolution. `name_resolver` then desugars both
//!   `DateTimeZone::listIdentifiers(...)` and `timezone_identifiers_list(...)` to
//!   `__elephc_list_identifiers(...)`.
//!
//! Key details:
//! - A free *function* is used (not a synthetic class method) on purpose: function
//!   return types are inferred flow-sensitively, so a built `array<string>` keeps
//!   its element type and `in_array`/`array_search`/`sort` work on the result. A
//!   synthetic method's built-array return would degrade to scalar `mixed` (see
//!   the `synthetic-method-return-inference-gap`), regressing `in_array`.
//! - Pay-for-use: injected only when `detect::program_uses_list_identifiers` finds
//!   a use, so non-datetime binaries never carry the ~13 KB baked table.
//! - The internal `$countryCode` default is `""` (not `null`): `=== null` on a
//!   null-defaulted param miscompiles, and the function is internal so no user
//!   observes the default. PER_COUNTRY with an empty country throws `ValueError`
//!   with PHP's exact message, matching the interpreter.

use crate::parser::ast::Program;

mod detect;
mod table;

/// The `__elephc_list_identifiers` source, with `__ELEPHC_TZ_GROUPS_TABLE__` as a
/// placeholder spliced with the baked table at injection time. `replace` is used
/// rather than `format!` so the PHP body's `{`/`}` need no escaping.
const LIST_ID_PRELUDE_TEMPLATE: &str = r#"<?php
function __elephc_list_identifiers($timezoneGroup = 2047, $countryCode = "") {
    $table = "__ELEPHC_TZ_GROUPS_TABLE__";
    $rows = explode(";", $table);
    $result = [];
    $perCountry = (($timezoneGroup & 4096) != 0);
    if ($perCountry && $countryCode === "") {
        throw new ValueError('DateTimeZone::listIdentifiers(): Argument #2 ($countryCode) must be a two-letter ISO 3166-1 compatible country code when argument #1 ($timezoneGroup) is DateTimeZone::PER_COUNTRY');
    }
    foreach ($rows as $row) {
        $f = explode(",", $row);
        $name = $f[0];
        if ($perCountry) {
            if ($f[2] === $countryCode) {
                $result[] = $name;
            }
        } else {
            $mask = (int) $f[1];
            if (($mask & $timezoneGroup) != 0) {
                $result[] = $name;
            }
        }
    }
    return $result;
}
"#;

/// Prepends the `__elephc_list_identifiers` function when the program references
/// `DateTimeZone::listIdentifiers` or `timezone_identifiers_list`; otherwise
/// returns the program unchanged so unrelated binaries pay nothing. The prelude is
/// hoisted function declarations only, so prepending does not change top-level
/// execution order. Tokenize/parse failure is a compiler bug and panics rather
/// than degrading silently.
pub fn inject_if_used(program: Program) -> Program {
    if !detect::program_uses_list_identifiers(&program) {
        return program;
    }
    let src = LIST_ID_PRELUDE_TEMPLATE.replace("__ELEPHC_TZ_GROUPS_TABLE__", table::TIMEZONE_GROUPS_TABLE);
    let tokens = crate::lexer::tokenize(&src).expect("list-id prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("list-id prelude must parse");
    combined.extend(program);
    combined
}
