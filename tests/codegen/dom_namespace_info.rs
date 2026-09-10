//! Purpose:
//! End-to-end regressions for PHP 8.5 modern DOM namespace-info value objects.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_namespace_info` through Rust's test harness.
//!
//! Key details:
//! - Namespace ordering, shadowing, canonical element identity, readonly slots, and cloning match php-src.

use crate::support::compile_and_run_capture;

/// Verifies in-scope and descendant namespace records are ordinary readonly PHP objects.
#[test]
fn namespace_info_matches_php_order_identity_and_value_object_semantics() {
    let out = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<r xmlns="urn:u0" xmlns:a="urn:u1" xmlns:b="urn:u2">'
    . '<c xmlns:c="urn:u3" xmlns:a="urn:u4" xmlns=""><g/></c>'
    . '</r>'
);
$root = $document->documentElement;
$child = $root->firstElementChild;
$grandchild = $child->firstElementChild;

$inScope = $child->getInScopeNamespaces();
$first = $inScope[0];
$second = $inScope[1];
$third = $inScope[2];
echo count($inScope), "|";
echo get_class($first), "|";
echo $first->prefix, "=", $first->namespaceURI, "|";
echo $first->element->nodeName, "|";
echo $first->element === $child ? "same" : "different", "\n";
$copy = clone $first;
echo $copy !== $first ? "clone" : "same-object", "|";
echo $copy->element === $first->element ? "same-element" : "different-element", "\n";
echo $second->prefix, "=", $second->namespaceURI, "|";
echo $third->prefix, "=", $third->namespaceURI, "|\n";

$descendants = $root->getDescendantNamespaces();
$rootDefault = $descendants[0];
$childInherited = $descendants[3];
$grandchildInherited = $descendants[6];
echo count($descendants), "|";
echo $rootDefault->prefix === null ? "NULL" : $rootDefault->prefix;
echo "|", $rootDefault->namespaceURI, "|", $rootDefault->element->nodeName, "\n";
echo $childInherited->prefix, "=", $childInherited->namespaceURI, "|";
echo $childInherited->element === $child ? "child" : "wrong", "\n";
echo $grandchildInherited->prefix, "=", $grandchildInherited->namespaceURI, "|";
echo $grandchildInherited->element === $grandchild ? "grandchild" : "wrong", "\n";

try {
    $first->prefix = "changed";
    echo "mutable\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}

$reflection = new ReflectionClass(Dom\NamespaceInfo::class);
echo $reflection->isFinal() ? "final\n" : "not-final\n";
echo $reflection->isReadOnly() ? "readonly\n" : "mutable\n";
echo $reflection->isInstantiable() ? "instantiable\n" : "not-instantiable\n";
echo $reflection->isCloneable() ? "cloneable\n" : "not-cloneable\n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={} stderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "3|Dom\\NamespaceInfo|b=urn:u2|c|same\n",
            "clone|same-element\n",
            "c=urn:u3|a=urn:u4|\n",
            "9|NULL|urn:u0|r\n",
            "b=urn:u2|child\n",
            "b=urn:u2|grandchild\n",
            "Cannot modify readonly property Dom\\NamespaceInfo::$prefix\n",
            "final\n",
            "readonly\n",
            "not-instantiable\n",
            "cloneable\n",
        )
    );
    assert_eq!(out.stderr, "");
}
