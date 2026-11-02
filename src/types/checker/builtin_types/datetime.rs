//! Purpose:
//! Facade for synthetic checker metadata covering PHP date/time classes and helpers.
//! Focused modules own timezone behavior, DateTime factories/setters, procedural
//! adapters, intervals, and final class-map injection.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`.
//! - `crate::types::checker::driver` initialization.
//!
//! Key details:
//! - Synthetic methods remain ordinary PHP AST lowered by the normal pipeline.
//! - Timezone bridge-dependent methods stay gated during declaration injection.

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind,
    TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::calendar;
use super::declarations::InterfaceDeclInfo;
use super::timezone_ids;

mod ast;
mod basics;
mod bodies;
mod create_from_format;
mod date_create;
mod datetime_serialize;
mod factories;
mod gate;
mod injection;
mod interface;
mod interval_constructor;
mod interval_diff;
mod interval_factory;
mod interval_format;
mod interval_serialize;
mod parse_formats;
mod parse_misc;
mod procedural_methods;
mod setter_helpers;
mod setters;
mod strftime;
mod strptime;
mod sun_sources;
mod timezone;

#[allow(unused_imports)]
use ast::*;
#[allow(unused_imports)]
use basics::*;
#[allow(unused_imports)]
use create_from_format::*;
#[allow(unused_imports)]
use date_create::*;
#[allow(unused_imports)]
use datetime_serialize::*;
#[allow(unused_imports)]
use factories::*;
#[allow(unused_imports)]
use interface::*;
#[allow(unused_imports)]
use interval_constructor::*;
#[allow(unused_imports)]
use interval_diff::*;
#[allow(unused_imports)]
use interval_factory::*;
#[allow(unused_imports)]
use interval_format::*;
#[allow(unused_imports)]
use interval_serialize::*;
#[allow(unused_imports)]
use parse_formats::*;
#[allow(unused_imports)]
use parse_misc::*;
#[allow(unused_imports)]
use procedural_methods::*;
#[allow(unused_imports)]
use setter_helpers::*;
#[allow(unused_imports)]
use setters::*;
#[allow(unused_imports)]
use strftime::*;
#[allow(unused_imports)]
use strptime::*;
#[allow(unused_imports)]
use sun_sources::*;
#[allow(unused_imports)]
use timezone::*;

pub(crate) use gate::program_may_reference_datetime;
pub(crate) use injection::inject_builtin_datetime;

#[cfg(test)]
mod bodies_oracle {
    use super::*;

    /// Every `(label, php, built)` triple the oracle sweeps.
    ///
    /// The four parameterized bodies appear TWICE, once per class name. The PHP side gets the same
    /// name through the `replace()` it used to rely on in production, so a single binding proves
    /// the transcription — but not that the builder reads its `class_name` at all, since a builder
    /// that hardcoded `"DateTime"` would match a PHP side where `replace()` had put `"DateTime"`.
    /// The second binding is what makes the parameter observable.
    fn cases() -> Vec<(&'static str, String, Vec<Stmt>)> {
        vec![
            ("CONSTRUCT_SRC", basics::CONSTRUCT_SRC.to_string(), bodies::construct()),
            ("FORMAT_SRC", basics::FORMAT_SRC.to_string(), bodies::format()),
            ("CREATE_FROM_FORMAT_SRC", create_from_format::CREATE_FROM_FORMAT_SRC.replace("__CFF_CLASS__", "DateTime"), bodies::create_from_format("DateTime")),
            ("GET_LAST_ERRORS_SRC", factories::GET_LAST_ERRORS_SRC.replace("__GLE_CLASS__", "DateTime"), bodies::get_last_errors("DateTime")),
            ("CREATE_FROM_OBJECT_SRC", factories::CREATE_FROM_OBJECT_SRC.replace("__TARGET__", "DateTime"), bodies::create_from_object("DateTime")),
            ("CREATE_FROM_TIMESTAMP_SRC", factories::CREATE_FROM_TIMESTAMP_SRC.replace("__CFT_CLASS__", "DateTime"), bodies::create_from_timestamp("DateTime")),
            // The SAME four bodies again, bound to the OTHER class. Binding one name on both sides
            // proves the transcription; it cannot prove the builder READS its argument, because a
            // builder that ignored `class_name` and hardcoded "DateTime" would match a PHP side
            // where `replace()` had put "DateTime" too. Two names make the parameter observable:
            // `DateTimeImmutable::createFromFormat` constructing a `DateTime` fails here rather
            // than surviving to be caught — or missed — by a behavioural test downstream.
            ("CREATE_FROM_FORMAT_SRC (Immutable)", create_from_format::CREATE_FROM_FORMAT_SRC.replace("__CFF_CLASS__", "DateTimeImmutable"), bodies::create_from_format("DateTimeImmutable")),
            ("GET_LAST_ERRORS_SRC (Immutable)", factories::GET_LAST_ERRORS_SRC.replace("__GLE_CLASS__", "DateTimeImmutable"), bodies::get_last_errors("DateTimeImmutable")),
            ("CREATE_FROM_OBJECT_SRC (Immutable)", factories::CREATE_FROM_OBJECT_SRC.replace("__TARGET__", "DateTimeImmutable"), bodies::create_from_object("DateTimeImmutable")),
            ("CREATE_FROM_TIMESTAMP_SRC (Immutable)", factories::CREATE_FROM_TIMESTAMP_SRC.replace("__CFT_CLASS__", "DateTimeImmutable"), bodies::create_from_timestamp("DateTimeImmutable")),
            ("SET_ISODATE_SRC", factories::SET_ISODATE_SRC.to_string(), bodies::set_isodate()),
            ("CREATE_FROM_DATE_STRING_SRC", interval_factory::CREATE_FROM_DATE_STRING_SRC.to_string(), bodies::create_from_date_string()),
            ("DATE_PARSE_FROM_FORMAT_SRC", parse_formats::DATE_PARSE_FROM_FORMAT_SRC.to_string(), bodies::date_parse_from_format()),
            ("DATE_PARSE_SRC", parse_misc::DATE_PARSE_SRC.to_string(), bodies::date_parse()),
            ("GETTIMEOFDAY_SRC", parse_misc::GETTIMEOFDAY_SRC.to_string(), bodies::gettimeofday()),
            ("MODIFY_PREAMBLE_SRC", setters::MODIFY_PREAMBLE_SRC.to_string(), bodies::modify_preamble()),
            ("STRFTIME_SRC", strftime::STRFTIME_SRC.to_string(), bodies::strftime()),
            ("EXTRACT_MICROS_SRC", strftime::EXTRACT_MICROS_SRC.to_string(), bodies::extract_micros()),
            ("STRIP_MICROS_SRC", strftime::STRIP_MICROS_SRC.to_string(), bodies::strip_micros()),
            ("NOW_MICROS_SRC", strftime::NOW_MICROS_SRC.to_string(), bodies::now_micros()),
            ("EXTRACT_MODIFY_MICROS_SRC", strftime::EXTRACT_MODIFY_MICROS_SRC.to_string(), bodies::extract_modify_micros()),
            ("STRIP_MODIFY_MICROS_SRC", strftime::STRIP_MODIFY_MICROS_SRC.to_string(), bodies::strip_modify_micros()),
            ("STRPTIME_SRC", strptime::STRPTIME_SRC.to_string(), bodies::strptime()),
            ("SUN_RS_SRC", sun_sources::SUN_RS_SRC.to_string(), bodies::sun_rs()),
            ("SUN_VAL_SRC", sun_sources::SUN_VAL_SRC.to_string(), bodies::sun_val()),
            ("SUN_INFO_SRC", sun_sources::SUN_INFO_SRC.to_string(), bodies::sun_info()),
            ("SUNFUNC_SRC", sun_sources::SUNFUNC_SRC.to_string(), bodies::sunfunc()),
            ("TZ_NAME_FROM_ABBR_SRC", sun_sources::TZ_NAME_FROM_ABBR_SRC.to_string(), bodies::tz_name_from_abbr()),
            ("GET_LOCATION_SRC", timezone::GET_LOCATION_SRC.to_string(), bodies::tz_get_location()),
            ("GET_TRANSITIONS_SRC", timezone::GET_TRANSITIONS_SRC.to_string(), bodies::tz_get_transitions()),
            ("LIST_ABBREVIATIONS_SRC", timezone::LIST_ABBREVIATIONS_SRC.to_string(), bodies::tz_list_abbreviations()),
        ]
    }

    /// THE ORACLE FOR THE TRANSCRIPTION: each built body must equal the parse of the PHP it
    /// replaced, statement by statement.
    ///
    /// These builders were generated by `synthetic_class::transcribe` and then reviewed, and
    /// neither step proves anything alone — a transcription that drops a qualifier or an argument
    /// still compiles and quietly means something else. This is what makes them safe to rely on,
    /// and it is why the PHP stays in the tree under `cfg(test)`.
    #[test]
    fn built_bodies_match_the_php() {
        for (label, php, built) in cases() {
            let tokens = crate::lexer::tokenize(&php)
                .unwrap_or_else(|e| panic!("{label} must tokenize: {e:?}"));
            let parsed = crate::parser::parse_internal(&tokens)
                .unwrap_or_else(|e| panic!("{label} must parse: {e:?}"));

            assert_eq!(
                built.len(),
                parsed.len(),
                "{label}: statement COUNT differs — built {} vs parsed {}",
                built.len(),
                parsed.len()
            );
            for (index, (built_stmt, parsed_stmt)) in built.iter().zip(parsed.iter()).enumerate() {
                assert_eq!(
                    strip_spans(&format!("{built_stmt:?}")),
                    strip_spans(&format!("{parsed_stmt:?}")),
                    "{label}: statement {index} differs from its PHP"
                );
            }
        }
    }

    /// `listIdentifiers()` has no PHP constant to diff — its body was FORMATTED from the
    /// identifier fragment and reparsed. This pins the two representations of that data together:
    /// the slice the builder reads and the PHP fragment the old path spliced must produce the
    /// same array literal, so neither can drift without the other.
    #[test]
    fn built_identifier_list_matches_the_php_fragment() {
        let php = format!(
            "<?php\nreturn [{}];\n",
            super::timezone_ids::TIMEZONE_IDENTIFIERS_ARRAY
        );
        let tokens = crate::lexer::tokenize(&php).expect("identifier fragment must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("identifier fragment must parse");
        let built = bodies::list_identifiers(super::timezone_ids::TIMEZONE_IDENTIFIERS);

        assert_eq!(built.len(), parsed.len(), "listIdentifiers: statement COUNT differs");
        assert_eq!(
            strip_spans(&format!("{:?}", built[0])),
            strip_spans(&format!("{:?}", parsed[0])),
            "listIdentifiers: the built array literal differs from the PHP fragment"
        );
    }

    /// Removes span payloads so a built node and a parsed node compare on structure alone.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }
}
