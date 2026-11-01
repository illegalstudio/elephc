//! Purpose:
//! End-to-end PHP compatibility coverage for the compiled `parse_str()` prelude.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.
//!
//! Key details:
//! - Every expectation below was measured against `php -n` 8.5 before it was written down; the
//!   grammar has enough documented quirks that guessing one is not safe.

use crate::support::*;

/// Parses the ordinary shapes: flat fields, repeats, `[]` appends and nested keys.
#[test]
fn test_parse_str_parses_flat_repeated_and_nested_fields() {
    let out = compile_and_run(
        r#"<?php
parse_str('a=1&b=2', $flat);
echo json_encode($flat), '|';
parse_str('a=1&a=2', $repeat);
echo json_encode($repeat), '|';
parse_str('a[]=1&a[]=2', $append);
echo json_encode($append), '|';
parse_str('a[b][c]=1', $nested);
echo json_encode($nested), '|';
parse_str('x[y][][z]=1&x[y][][z]=2', $deep);
echo json_encode($deep);
"#,
    );
    assert_eq!(
        out,
        r#"{"a":"1","b":"2"}|{"a":"2"}|{"a":["1","2"]}|{"a":{"b":{"c":"1"}}}|{"x":{"y":[{"z":"1"},{"z":"2"}]}}"#
    );
}

/// Replaces the destination array outright, and creates it when the caller never declared it.
#[test]
fn test_parse_str_replaces_its_output_and_creates_an_undeclared_one() {
    let out = compile_and_run(
        r#"<?php
$stale = ['keep' => 'me'];
parse_str('a=1', $stale);
echo json_encode($stale), '|';
parse_str('b=2', $fresh);
echo json_encode($fresh), '|';
parse_str('', $empty);
echo json_encode($empty);
"#,
    );
    assert_eq!(out, r#"{"a":"1"}|{"b":"2"}|[]"#);
}

/// Decodes `+` and `%XX` once per component, leaving an incomplete escape literal.
#[test]
fn test_parse_str_decodes_form_components() {
    let out = compile_and_run(
        r#"<?php
parse_str('name=Jane+Doe&other=Jane%20Doe', $decoded);
echo json_encode($decoded), '|';
parse_str('x=%41%42%43&short=%4&bad=%zz&trailing=100%', $escapes);
echo json_encode($escapes), '|';
parse_str('a=b%26c=d', $ampersand);
echo json_encode($ampersand), '|';
parse_str('a%5Bb%5D=1', $bracketed);
echo json_encode($bracketed);
"#,
    );
    assert_eq!(
        out,
        r#"{"name":"Jane Doe","other":"Jane Doe"}|{"x":"ABC","short":"%4","bad":"%zz","trailing":"100%"}|{"a":"b&c=d"}|{"a":{"b":"1"}}"#
    );
}

/// Mangles `' '` and `'.'` in the ROOT only, and strips leading spaces from the name.
#[test]
fn test_parse_str_mangles_the_root_name_only() {
    let out = compile_and_run(
        r#"<?php
parse_str('a.b=1&c d=2', $root);
echo json_encode($root), '|';
parse_str('a[x.y]=1&a[x y]=2', $segments);
echo json_encode($segments), '|';
parse_str(' a=1', $leading);
echo json_encode($leading), '|';
parse_str('  a  b =1', $inner);
echo json_encode($inner), '|';
parse_str("\ta=1", $tab);
echo json_encode($tab);
"#,
    );
    assert_eq!(
        out,
        r#"{"a_b":"1","c_d":"2"}|{"a":{"x.y":"1","x y":"2"}}|{"a":"1"}|{"a__b_":"1"}|{"\ta":"1"}"#
    );
}

/// Drops a field with an empty root, and skips empty fields around separators.
#[test]
fn test_parse_str_drops_empty_names_and_fields() {
    let out = compile_and_run(
        r#"<?php
parse_str('=1&[a]=2&[]=3&a=4', $dropped);
echo json_encode($dropped), '|';
parse_str('&&a=1&&', $separators);
echo json_encode($separators), '|';
parse_str('a', $novalue);
echo json_encode($novalue), '|';
parse_str('a=', $emptyvalue);
echo json_encode($emptyvalue);
"#,
    );
    assert_eq!(out, r#"{"a":"4"}|{"a":"1"}|{"a":""}|{"a":""}"#);
}

/// Applies php's two DIFFERENT recoveries for an unterminated `[`.
#[test]
fn test_parse_str_recovers_from_unterminated_brackets() {
    let out = compile_and_run(
        r#"<?php
parse_str('a[=1', $first);
echo json_encode($first), '|';
parse_str('a[b=1', $firstNamed);
echo json_encode($firstNamed), '|';
parse_str('a[b][=1', $later);
echo json_encode($later), '|';
parse_str('a[b][c=1', $laterNamed);
echo json_encode($laterNamed), '|';
parse_str('a[b]c[d]=1', $trailing);
echo json_encode($trailing), '|';
parse_str('a[b]]=1', $extraClose);
echo json_encode($extraClose);
"#,
    );
    assert_eq!(
        out,
        r#"{"a_":"1"}|{"a_b":"1"}|{"a":{"b":"1"}}|{"a":{"b":"1"}}|{"a":{"b":"1"}}|{"a":{"b":"1"}}"#
    );
}

/// Appends at `max(existing int keys) + 1`, negative keys included, and treats a lone space or
/// tab segment as an append too.
#[test]
fn test_parse_str_append_key_follows_existing_integer_keys() {
    let out = compile_and_run(
        r#"<?php
parse_str('a[]=1&a[3]=x&a[]=2', $afterHigher);
echo json_encode($afterHigher), '|';
parse_str('a[-5]=x&a[]=y', $afterNegative);
echo json_encode($afterNegative), '|';
parse_str('a[01]=x&a[]=y', $nonCanonical);
echo json_encode($nonCanonical), '|';
parse_str('a[-0]=x&a[]=y', $negativeZero);
echo json_encode($negativeZero), '|';
parse_str('a[ ]=1&a[ ]=2', $spaceSegment);
echo json_encode($spaceSegment), '|';
parse_str("a[\t]=1", $tabSegment);
echo json_encode($tabSegment);
"#,
    );
    assert_eq!(
        out,
        r#"{"a":{"0":"1","3":"x","4":"2"}}|{"a":{"-5":"x","-4":"y"}}|{"a":{"01":"x","0":"y"}}|{"a":{"-0":"x","0":"y"}}|{"a":["1","2"]}|{"a":["1"]}"#
    );
}

/// Reuses an existing array at a key and replaces anything else, in BOTH directions.
#[test]
fn test_parse_str_reuses_or_replaces_an_existing_value() {
    let out = compile_and_run(
        r#"<?php
parse_str('a=1&a[b]=2', $scalarThenArray);
echo json_encode($scalarThenArray), '|';
parse_str('a[b]=1&a=2', $arrayThenScalar);
echo json_encode($arrayThenScalar), '|';
parse_str('a[b]=1&a[b][c]=2', $deepenLeaf);
echo json_encode($deepenLeaf), '|';
parse_str('a[b][c]=1&a[b]=2', $flattenBranch);
echo json_encode($flattenBranch);
"#,
    );
    assert_eq!(
        out,
        r#"{"a":{"b":"2"}}|{"a":"2"}|{"a":{"b":{"c":"2"}}}|{"a":{"b":"2"}}"#
    );
}

/// Stops the WHOLE parse at the first NUL byte, not just the field carrying it.
#[test]
fn test_parse_str_stops_at_the_first_nul_byte() {
    let out = compile_and_run(
        r#"<?php
parse_str("a=1\0b=2", $cutBetween);
echo json_encode($cutBetween), '|';
parse_str("a\0b=1", $cutInsideName);
echo json_encode($cutInsideName);
"#,
    );
    assert_eq!(out, r#"{"a":"1"}|{"a":""}"#);
}

/// Resolves an UNQUALIFIED call inside a namespace through php's global-function fallback.
///
/// Symfony's `EnvVarProcessor` calls `parse_str()` bare from
/// `namespace Symfony\Component\DependencyInjection`, so without the fallback the compiled
/// program reported `Call to undefined function Symfony\...\parse_str()`.
#[test]
fn test_parse_str_resolves_unqualified_inside_a_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace Symfony\Component\DependencyInjection;

function probe(string $queryString): array
{
    parse_str($queryString, $result);

    return $result;
}

echo \json_encode(probe('a=1&b[]=2&b[]=3')), '|';
echo \json_encode(\Symfony\Component\DependencyInjection\probe('c=3'));
"#,
    );
    assert_eq!(out, r#"{"a":"1","b":["2","3"]}|{"c":"3"}"#);
}
