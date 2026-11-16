//! Purpose:
//! Bounds SimpleXML declaration reachability without weakening dynamic XML callbacks.
//!
//! Called from:
//! - Library CI tests for production declaration pruning.
//!
//! Key details:
//! - Uses the actual injected XML prelude, checker metadata, and reachability graph.
//! - Explicit XML forcing and opaque eval retain their complete capabilities.

use std::collections::HashSet;

use super::{graph, usage, PreludeInventory, PruneOptions};
use crate::names::php_symbol_key;

/// Computes production reachability after injecting and checking the complete XML surface.
fn xml_reachability(source: &str, force: bool) -> graph::Reachability {
    let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
    let ast = crate::parser::parse(&tokens).expect("fixture must parse");
    let mut inventory = PreludeInventory::new();
    let ast = crate::core_prelude::inject_if_used(ast, &mut inventory);
    let ast = crate::xml_prelude::inject_if_used(ast, true, &mut inventory);
    let ast = crate::dom_html_prelude::inject_if_used(ast, false, &mut inventory);
    let ast = crate::name_resolver::resolve(ast).expect("fixture must resolve");
    let ast = crate::func_args::desugar(ast).expect("fixture must desugar");
    let ast = crate::optimize::fold_constants(ast);
    let check = crate::types::check(&ast).expect("fixture must type check");
    let forced = if force { HashSet::from(["xml".to_string()]) } else { HashSet::new() };
    graph::compute(&ast, &check, &PruneOptions {
        inventory: &inventory,
        forced_groups: &forced,
        exported_functions: &HashSet::new(),
        eval_forced: false,
    })
}

/// Both constructor and runtime-name factory paths retain the XML family, not unrelated classes.
#[test]
fn simplexml_factories_keep_only_compatible_class_families() {
    for expression in ["new SimpleXMLElement('<r/>')", "simplexml_load_string('<r/>', $argv[1])"] {
        let reach = xml_reachability(&format!("<?php class CompatibleXml extends SimpleXMLElement {{}} class UnrelatedXml {{}} $xml = {expression}; echo (string)$xml->node;"), false);
        for class in ["SimpleXMLElement", "SimpleXMLIterator", "CompatibleXml"] {
            assert!(reach.classes.contains(&php_symbol_key(class)), "{expression}: {class}");
        }
        assert!(!reach.classes.contains(&php_symbol_key("UnrelatedXml")), "{expression}");
        assert!(!reach.hazards.dynamic_class, "{expression}");
        assert!(!reach.hazards.dynamic_function, "{expression}");
    }
}

/// Literal XML-only eval keeps the bridge sentinel without importing SAX callback hazards.
#[test]
fn simplexml_literal_eval_does_not_root_unused_parser_callbacks() {
    let reach = xml_reachability("<?php $xml = new SimpleXMLElement('<r/>'); eval('echo (string)$xml->node;');", false);
    assert!(reach.functions.contains(&php_symbol_key("xml_parser_create")));
    assert!(!reach.functions.contains(&php_symbol_key("xml_parse")));
    assert!(!reach.classes.contains(&php_symbol_key("XMLWriter")));
    assert!(!reach.hazards.dynamic_function);
    assert!(!reach.hazards.dynamic_class);
}

/// Literal parser calls retain the actual user-callable dispatch rather than suppressing its hazards.
#[test]
fn simplexml_literal_parser_eval_keeps_user_callback_hazards() {
    let reach = xml_reachability("<?php eval('$parser = xml_parser_create(); xml_set_character_data_handler($parser, $argv[1]); xml_parse($parser, \"<r/>\", true);');", false);
    assert!(reach.functions.contains(&php_symbol_key("xml_parse")));
    assert!(reach.hazards.dynamic_function);
    assert!(reach.hazards.dynamic_method);
}

/// Forced XML and opaque eval keep every parser/writer function and unrelated user class.
#[test]
fn simplexml_forced_and_opaque_capabilities_remain_conservative() {
    for (source, force) in [
        ("<?php echo 1;", true),
        ("<?php class UnrelatedXml {} eval($argv[1]);", false),
    ] {
        let reach = xml_reachability(source, force);
        for function in ["xml_parse", "xmlwriter_open_memory", "simplexml_load_string"] {
            assert!(reach.functions.contains(&php_symbol_key(function)), "{source}: {function}");
        }
        if !force { assert!(reach.classes.contains(&php_symbol_key("UnrelatedXml"))); }
    }
}

/// Arbitrary internal allocation and ordinary string probes still retain dynamic class lookup.
#[test]
fn simplexml_family_precision_does_not_narrow_unvalidated_calls() {
    for source in [
        "<?php __elephc_simplexml_allocate($argv[1]);",
        "<?php is_a($argv[1], 'SimpleXMLElement', true);",
        "<?php function user_probe($name) { return is_a($name, 'SimpleXMLElement', true); }",
    ] {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let ast = crate::parser::parse_internal(&tokens).unwrap();
        assert!(usage::scan_program(&ast).hazards.dynamic_class, "{source}");
    }
}
