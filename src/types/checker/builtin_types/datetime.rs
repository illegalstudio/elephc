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
//! - Synthetic methods are direct Rust-built PHP AST lowered by the normal pipeline.
//! - Parser-backed PHP models are test-only generation oracles, never production inputs.
//! - Timezone bridge-dependent methods stay gated during declaration injection.

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
use crate::names::{Name, NameKind};
#[cfg(test)]
use crate::parser::ast::{
    Attribute, AttributeGroup, BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind,
    PropertyHooks, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
#[cfg(test)]
use crate::types::traits::FlattenedClass;

#[cfg(test)]
use super::calendar;
#[cfg(test)]
use super::declarations::InterfaceDeclInfo;
#[cfg(test)]
use super::timezone_ids;

#[cfg(test)]
#[allow(dead_code)]
mod ast;
#[cfg(test)]
#[allow(dead_code)]
mod basics;
#[cfg(test)]
#[allow(dead_code)]
mod bodies;
#[cfg(test)]
#[allow(dead_code)]
mod create_from_format;
#[cfg(test)]
#[allow(dead_code)]
mod factories;
mod gate;
mod generated_declarations_fallback;
mod generated_declarations_timelib;
mod generated_injection;
#[cfg(test)]
#[allow(dead_code)]
mod injection;
#[cfg(test)]
#[allow(dead_code)]
mod interface;
#[cfg(test)]
#[allow(dead_code)]
mod interval_constructor;
#[cfg(test)]
#[allow(dead_code)]
mod interval_diff;
#[cfg(test)]
#[allow(dead_code)]
mod interval_factory;
#[cfg(test)]
#[allow(dead_code)]
mod interval_format;
#[cfg(test)]
#[allow(dead_code)]
mod parse_formats;
#[cfg(test)]
#[allow(dead_code)]
mod parse_misc;
#[cfg(test)]
#[allow(dead_code)]
mod procedural_methods;
#[cfg(test)]
#[allow(dead_code)]
mod setter_helpers;
#[cfg(test)]
#[allow(dead_code)]
mod setters;
#[cfg(test)]
#[allow(dead_code)]
mod strftime;
#[cfg(test)]
#[allow(dead_code)]
mod strptime;
#[cfg(test)]
#[allow(dead_code)]
mod sun_sources;
#[cfg(test)]
#[allow(dead_code)]
mod timezone;
#[cfg(test)]
#[allow(dead_code)]
mod compliance_core;
#[cfg(test)]
#[allow(dead_code)]
mod compliance_methods;
#[cfg(test)]
#[allow(dead_code)]
mod compliance_procedural;
#[cfg(test)]
#[allow(dead_code)]
mod compliance_interval;

#[cfg(test)]
use ast::*;
#[cfg(test)]
use basics::*;
#[cfg(test)]
use create_from_format::*;
#[cfg(test)]
use factories::*;
#[cfg(test)]
use interface::*;
#[cfg(test)]
use interval_constructor::*;
#[cfg(test)]
use interval_diff::*;
#[cfg(test)]
use interval_factory::*;
#[cfg(test)]
use interval_format::*;
#[cfg(test)]
use parse_formats::*;
#[cfg(test)]
use parse_misc::*;
#[cfg(test)]
use procedural_methods::*;
#[cfg(test)]
use setter_helpers::*;
#[cfg(test)]
use setters::*;
#[cfg(test)]
use strftime::*;
#[cfg(test)]
use strptime::*;
#[cfg(test)]
use sun_sources::*;
#[cfg(test)]
use timezone::*;

pub(crate) use gate::{program_may_reference_date_period, program_may_reference_datetime};
#[cfg(test)]
pub(super) use compliance_core::deprecated_attribute;
pub(crate) use generated_injection::{inject_builtin_date_period, inject_builtin_datetime};

#[cfg(test)]
#[allow(dead_code)]
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
            (
                "CONSTRUCT_SRC",
                compliance_core::CONSTRUCT_SRC.to_string(),
                bodies::construct(),
            ),
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

    /// Converts one checked builtin class description into a transcribable AST declaration.
    fn class_declaration(class: FlattenedClass) -> Stmt {
        assert!(class.used_traits.is_empty(), "generated date classes must not use traits");
        assert!(class.trait_aliases.is_empty(), "generated date classes must not use aliases");
        Stmt::with_attributes(
            StmtKind::ClassDecl {
                name: class.name,
                extends: class.extends.map(Name::unqualified),
                implements: class
                    .implements
                    .into_iter()
                    .map(Name::unqualified)
                    .collect(),
                is_abstract: class.is_abstract,
                is_final: class.is_final,
                is_readonly_class: class.is_readonly_class,
                trait_uses: Vec::new(),
                properties: class.properties,
                methods: class.methods,
                constants: class.constants,
            },
            class.span,
            class.attributes,
        )
    }

    /// Converts one checked builtin interface description into a transcribable AST declaration.
    fn interface_declaration(info: super::super::declarations::InterfaceDeclInfo) -> Stmt {
        Stmt::new(
            StmtKind::InterfaceDecl {
                name: info.name,
                extends: info.extends.into_iter().map(Name::unqualified).collect(),
                properties: info.properties,
                methods: info.methods,
                constants: info.constants,
            },
            info.span,
        )
    }

    /// Builds the audited parser-backed declaration model used only by generator/oracle tests.
    fn audited_datetime_declarations(uses_timelib: bool) -> Vec<Stmt> {
        let mut interfaces = HashMap::new();
        let mut classes = HashMap::new();
        super::compliance_interval::inject_builtin_datetime(
            &mut interfaces,
            &mut classes,
            uses_timelib,
        );
        super::super::date_period::compliance_state::inject_builtin_date_period(
            &mut classes,
            uses_timelib,
        );

        let mut declarations = Vec::new();
        declarations.push(interface_declaration(
            interfaces
                .remove("DateTimeInterface")
                .expect("DateTimeInterface must be generated"),
        ));
        for class_name in [
            "DateInterval",
            "DatePeriod",
            "DateTime",
            "DateTimeImmutable",
            "DateTimeZone",
        ] {
            declarations.push(class_declaration(
                classes
                    .remove(class_name)
                    .unwrap_or_else(|| panic!("{class_name} must be generated")),
            ));
        }
        declarations
    }

    /// Generates one direct-AST DateTime declaration variant into `output_path`.
    fn generate_direct_datetime_declarations(output_path: &str, uses_timelib: bool) {
        let declarations = audited_datetime_declarations(uses_timelib);
        let generated = crate::synthetic_class::transcribe::transcribe_split_plain(
            &declarations,
            "generated_datetime_declarations",
        );
        let source = format!(
            "//! Purpose:\n//! Direct AST declarations for the php-src-compatible DateTime family.\n//!\n//! Called from:\n//! - The DateTime builtin declaration injector.\n//!\n//! Key details:\n//! - Generated in tests from the audited declaration model; production performs no PHP parsing.\n\nuse crate::parser::ast::*;\nuse crate::synthetic_class::*;\n\n{generated}"
        );
        std::fs::write(output_path, source).expect("generated DateTime declarations must write");
    }

    /// Generates both production direct-AST date variants when explicit output paths are set.
    #[test]
    fn generate_direct_datetime_declarations_on_request() {
        if let Ok(output_path) = std::env::var("ELEPHC_GENERATED_DATETIME_TIMELIB_OUT") {
            generate_direct_datetime_declarations(&output_path, true);
        }
        if let Ok(output_path) = std::env::var("ELEPHC_GENERATED_DATETIME_FALLBACK_OUT") {
            generate_direct_datetime_declarations(&output_path, false);
        }
    }

    /// Proves both checked-in generated variants match the audited test-only declaration model.
    #[test]
    fn generated_datetime_declarations_match_audited_models() {
        for (label, uses_timelib, mut generated) in [
            (
                "timelib",
                true,
                super::generated_declarations_timelib::generated_datetime_declarations(),
            ),
            (
                "fallback",
                false,
                super::generated_declarations_fallback::generated_datetime_declarations(),
            ),
        ] {
            let mut audited = audited_datetime_declarations(uses_timelib);
            assert_eq!(generated.len(), audited.len(), "{label} declaration count");
            normalize_empty_param_attributes(&mut generated);
            normalize_empty_param_attributes(&mut audited);
            let generated_debug = format!("{generated:#?}");
            assert!(
                !generated_debug.contains("source_mode: Php"),
                "{label} generated declarations must remain internal at every nesting level"
            );
            let generated_ast = strip_source_modes(&strip_spans(&generated_debug));
            let audited_ast = strip_source_modes(&strip_spans(&format!("{audited:#?}")));
            assert_text_lines_match(label, "AST structure", &generated_ast, &audited_ast);
            let generated_php = crate::synthetic_class::print::print_program(&generated);
            let audited_php = crate::synthetic_class::print::print_program(&audited);
            if generated_php != audited_php {
                let mismatch = generated_php
                    .bytes()
                    .zip(audited_php.bytes())
                    .position(|(left, right)| left != right)
                    .unwrap_or_else(|| generated_php.len().min(audited_php.len()));
                let start = mismatch.saturating_sub(160);
                let generated_end = (mismatch + 320).min(generated_php.len());
                let audited_end = (mismatch + 320).min(audited_php.len());
                panic!(
                    "{label} declarations drifted at byte {mismatch}\ngenerated: {}\naudited: {}",
                    &generated_php[start..generated_end],
                    &audited_php[start..audited_end],
                );
            }
        }
    }

    /// Verifies both generated variants construct and drop within Rust's standard libtest stack.
    #[test]
    fn generated_datetime_declarations_fit_standard_libtest_stack() {
        for build in [
            super::generated_declarations_timelib::generated_datetime_declarations
                as fn() -> crate::parser::ast::Program,
            super::generated_declarations_fallback::generated_datetime_declarations,
        ] {
            std::thread::Builder::new()
                .stack_size(2 * 1024 * 1024)
                .spawn(move || {
                    let declarations = build();
                    assert_eq!(declarations.len(), 6);
                    drop(declarations);
                })
                .expect("generated DateTime stack probe must spawn")
                .join()
                .expect("generated DateTime declarations must fit the standard worker stack");
        }
    }

    /// Canonicalizes semantically empty parameter-attribute padding before AST comparison.
    fn normalize_empty_param_attributes(program: &mut crate::parser::ast::Program) {
        for stmt in program {
            match &mut stmt.kind {
                StmtKind::ClassDecl { methods, .. } | StmtKind::InterfaceDecl { methods, .. } => {
                    for method in methods {
                        if method.param_attributes.iter().all(Vec::is_empty) {
                            method.param_attributes.clear();
                        }
                    }
                }
                StmtKind::FunctionDecl {
                    param_attributes, ..
                } if param_attributes.iter().all(Vec::is_empty) => param_attributes.clear(),
                _ => {}
            }
        }
    }

    /// Reports the first line of generated declaration drift without dumping the whole AST.
    fn assert_text_lines_match(label: &str, surface: &str, generated: &str, audited: &str) {
        let generated_lines: Vec<&str> = generated.lines().collect();
        let audited_lines: Vec<&str> = audited.lines().collect();
        let line_count = generated_lines.len().max(audited_lines.len());
        for line_index in 0..line_count {
            let generated_line = generated_lines.get(line_index).copied().unwrap_or("<missing>");
            let audited_line = audited_lines.get(line_index).copied().unwrap_or("<missing>");
            assert_eq!(
                generated_line,
                audited_line,
                "{label} declaration {surface} drifted at line {}",
                line_index + 1
            );
        }
    }

    /// Removes legacy source-mode stamps that differ only because old helpers built bodies early.
    fn strip_source_modes(rendered: &str) -> String {
        rendered
            .lines()
            .filter(|line| !line.trim_start().starts_with("source_mode:"))
            .collect::<Vec<_>>()
            .join("\n")
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
