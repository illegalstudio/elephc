//! Purpose:
//! Regression coverage for interface method reflection over layered diamond graphs.
//!
//! Called from:
//! - The reflection method-name module's unit test harness.
//!
//! Key details:
//! - Exercises metadata traversal independently of nested ReflectionClass object emission.

use super::*;

/// Shared ancestors are visited once while preserving declared, static, and inherited order.
#[test]
fn layered_diamond_visits_each_interface_once() {
    let mut source = String::from("<?php\ninterface Root { public function base(); }\n");
    let mut parents = String::from("Root");
    for layer in 0..24 {
        source.push_str(&format!(
            "interface Left{layer} extends {parents} {{}}\ninterface Right{layer} extends {parents} {{}}\n"
        ));
        parents = format!("Left{layer}, Right{layer}");
    }
    source.push_str(&format!(
        "interface Leaf extends {parents} {{ public static function first(); public function last(); }}"
    ));
    let tokens = crate::lexer::tokenize(&source).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let checked = crate::types::check(&program).unwrap();
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    collect_reflection_interface_method_names(
        &checked.interfaces, "Leaf", &mut names, &mut seen, &mut visited,
    );
    assert_eq!(names, ["first", "last", "base"]);
    assert_eq!(visited.len(), 50);
    names.clear();
    seen.clear();
    collect_reflection_interface_method_names(
        &checked.interfaces, "Leaf", &mut names, &mut seen, &mut visited,
    );
    assert!(names.is_empty(), "visited interfaces must not be traversed again");
}
