//! Purpose:
//! Verifies array literals store nullsafe-chain results using their boxed EIR representation.
//!
//! Called from:
//! - The indexed array codegen integration module.
//!
//! Key details:
//! - Indexed and associative literals preserve both object and null result tags.
//! - Ordinary postfix segments after a nullsafe segment still belong to the boxed chain.

use crate::support::*;

/// Nested nullable property and method chains survive list/map insertion without integer coercion.
#[test]
fn test_nullsafe_array_literals_preserve_object_and_null_tags() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LiteralLink { public function __construct(public string $label) {} }
class LiteralFactory {
    public function link(): LiteralLink { return new LiteralLink(str_repeat("L", 3)); }
}
class LiteralHolder { public function __construct(public ?LiteralFactory $factory) {} }
function listResult(?LiteralHolder $holder): array { return [$holder?->factory?->link()]; }
function mapResult(?LiteralHolder $holder): array { return ["link" => $holder?->factory?->link()]; }
function propertyResult(?LiteralHolder $holder): array { return [$holder?->factory]; }
$holder = new LiteralHolder(new LiteralFactory());
$present = listResult($holder);
$map = mapResult($holder);
echo $present[0]->label, ":", $map["link"]->label, "|";
echo gettype(listResult(null)[0]), ":", gettype(mapResult(null)["link"]), "|";
echo gettype(listResult(new LiteralHolder(null))[0]), "|";
echo gettype(propertyResult($holder)[0]), ":", gettype(propertyResult(null)[0]);
unset($holder, $present, $map);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "LLL:LLL|NULL:NULL|NULL|object:NULL", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Non-nullable receivers and ordinary property/index suffixes still produce boxed chain results.
#[test]
fn test_nullsafe_array_literals_preserve_scalar_suffixes_and_nonnullable_receivers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SuffixLiteral {
    public function __construct(public string $name) {}
    public function names(): array { return [$this->name]; }
    public function identity(): SuffixLiteral { return $this; }
}
function suffixes(?SuffixLiteral $value): array {
    return [$value?->identity()->name, $value?->names()[0]];
}
function nonnullable(SuffixLiteral $value): array { return [$value?->identity(), "name" => $value?->name]; }
$value = new SuffixLiteral(str_repeat("S", 3));
$data = suffixes($value);
$empty = suffixes(null);
$object = nonnullable($value);
echo $data[0], ":", $data[1], "|", gettype($empty[0]), ":", gettype($empty[1]), "|";
echo $object[0]->name, ":", $object["name"];
unset($value, $data, $empty, $object);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "SSS:SSS|NULL:NULL|SSS:SSS", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
