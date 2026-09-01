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
//!   observes the default. A hidden entry-point label keeps procedural and OOP
//!   validation messages exact without duplicating the baked filtering table.

use crate::parser::ast::{BinOp, CastType, Program};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_cast, e_index, e_int, e_new, e_str, e_var, function,
    internal_declarations, s_array_push, s_assign, s_foreach, s_if, s_return, s_throw,
};

mod detect;
mod table;

/// Shared suffix of the `ValueError` raised for an invalid `PER_COUNTRY` country code.
const INVALID_PER_COUNTRY_SUFFIX: &str = "(): Argument #2 ($countryCode) must be a two-letter ISO 3166-1 compatible country code when argument #1 ($timezoneGroup) is DateTimeZone::PER_COUNTRY";

/// Builds `__elephc_list_identifiers`, which filters the baked timezone table by group mask
/// or by country.
///
/// The parameters are UNTYPED, as in the PHP form: the internal `$countryCode` default is
/// `""` rather than `null` because `=== null` on a null-defaulted parameter miscompiles, and
/// the function is internal so no user observes the default.
///
/// The table used to be spliced into the source text through a `__ELEPHC_TZ_GROUPS_TABLE__`
/// placeholder. It is now simply the string literal the body reads, so there is no
/// placeholder to collide with table content and no escaping question.
pub(crate) fn list_id_declarations() -> Program {
    internal_declarations(|| {
        vec![function("__elephc_list_identifiers")
            .param_untyped_default("timezoneGroup", e_int(2047))
            .param_untyped_default("countryCode", e_str(""))
            .param_untyped_default("entryPoint", e_str("DateTimeZone::listIdentifiers"))
            .body(vec![
                s_assign("table", e_str(table::TIMEZONE_GROUPS_TABLE)),
                s_assign(
                    "rows",
                    e_call("explode", vec![e_str(";"), e_var("table")]),
                ),
                s_assign("result", e_array(vec![])),
                s_assign(
                    "perCountry",
                    e_binop(
                        e_binop(e_var("timezoneGroup"), BinOp::BitAnd, e_int(4096)),
                        BinOp::NotEq,
                        e_int(0),
                    ),
                ),
                s_if(
                    e_binop(
                        e_var("perCountry"),
                        BinOp::And,
                        e_binop(
                            e_call(
                                "strlen",
                                vec![e_cast(CastType::String, e_var("countryCode"))],
                            ),
                            BinOp::StrictNotEq,
                            e_int(2),
                        ),
                    ),
                    vec![s_throw(e_new(
                        "ValueError",
                        vec![e_binop(
                            e_var("entryPoint"),
                            BinOp::Concat,
                            e_str(INVALID_PER_COUNTRY_SUFFIX),
                        )],
                    ))],
                    vec![],
                    None,
                ),
                s_foreach(
                    e_var("rows"),
                    None,
                    "row",
                    vec![
                        s_assign("f", e_call("explode", vec![e_str(","), e_var("row")])),
                        s_assign("name", e_index(e_var("f"), e_int(0))),
                        s_if(
                            e_var("perCountry"),
                            vec![s_if(
                                e_binop(
                                    e_index(e_var("f"), e_int(2)),
                                    BinOp::StrictEq,
                                    e_var("countryCode"),
                                ),
                                vec![s_array_push("result", e_var("name"))],
                                vec![],
                                None,
                            )],
                            vec![],
                            Some(vec![
                                s_assign(
                                    "mask",
                                    e_cast(CastType::Int, e_index(e_var("f"), e_int(1))),
                                ),
                                s_if(
                                    e_binop(
                                        e_binop(
                                            e_var("mask"),
                                            BinOp::BitAnd,
                                            e_var("timezoneGroup"),
                                        ),
                                        BinOp::NotEq,
                                        e_int(0),
                                    ),
                                    vec![s_array_push("result", e_var("name"))],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ),
                    ],
                ),
                s_return(e_var("result")),
            ])
            .build()]
    })
}

/// Prepends the `__elephc_list_identifiers` function when the program references
/// `DateTimeZone::listIdentifiers` or `timezone_identifiers_list`; otherwise
/// returns the program unchanged so unrelated binaries pay nothing. The prelude is
/// hoisted function declarations only, so prepending does not change top-level
/// execution order.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !detect::program_uses_list_identifiers(&program) {
        return program;
    }
    let mut combined = list_id_declarations();
    inventory.record_program("list_id", &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// All three parameters stay UNTYPED. A hint here would change how the checker infers the
    /// callers' argument types, which is a signature change rather than a transcription.
    #[test]
    fn all_parameters_stay_untyped() {
        let decl = list_id_declarations()
            .into_iter()
            .next()
            .expect("one declaration");
        let StmtKind::FunctionDecl { params, .. } = &decl.kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(params.len(), 3);
        assert!(params.iter().all(|(_, ty, _, _)| ty.is_none()));
    }
}
