//! Purpose:
//! Exercises public mb_parse_str calls through native and opaque eval reference adapters.
//!
//! Called from:
//! - The focused codegen string test module.
//!
//! Key details:
//! - Fixtures observe nested writes, aliases, named arguments, and shared request identification.
//! - Heap checks include the temporary source and the replaced output value.

use crate::support::*;

/// Parses binary, nested, repeated, and normalized keys through a case-insensitive namespaced call.
#[test]
fn test_mbstring_parse_str_native() {
    let source = r#"<?php
namespace QueryCalls;
var_dump(Mb_Parse_Str("name=Caf%C3%A9&list[]=one&list[]=two&nested[key]=value&dot.key=a%00b&name=last", $result));
echo $result["name"], ":", $result["list"][0], ":", $result["list"][1], ":", $result["nested"]["key"], "\n";
echo bin2hex((string)$result["dot_key"]), "\n";
$alias =& $result;
var_dump(\mb_parse_str("new=value", $alias));
echo count($result), ":", $result["new"], "\n";
"#;
    clean(source, "bool(true)\nlast:one:two:value\n610062\nbool(true)\n1:value\n");
}

/// Keeps the output reference visible while later named source evaluation replaces its prior value.
#[test]
fn test_mbstring_parse_str_named_arguments() {
    let source = r#"<?php
function query_source(mixed &$value): string { $value = 42; echo "source\n"; return "answer=done"; }
$result = "old";
var_dump(mb_parse_str(result: $result, string: query_source($result)));
echo $result["answer"], "\n";
"#;
    clean(source, "source\nbool(true)\ndone\n");
}

/// Uses the actual shared V5 runtime in opaque eval, including a reference parameter and an alias.
#[test]
fn test_mbstring_parse_str_eval() {
    let source = r#"<?php
mb_internal_encoding("UTF-8");
$source = $argc > 0 ? '
function query_into(mixed &$output): bool { return mb_parse_str("key=value&list[]=a&list[]=b", $output); }
$result = 42; $alias =& $result;
var_dump(query_into($alias));
echo $result["key"], ":", $alias["list"][1], "\n";
var_dump(mb_parse_str(result: $result, string: "other=next"));
echo count($alias), ":", $alias["other"], "\n";
' : '';
eval($source);
"#;
    clean(source, "bool(true)\nvalue:b\nbool(true)\n1:next\n");
}

/// Retires each prior concrete local representation when query output promotes it to a reference.
#[test]
fn test_mbstring_parse_str_initialized_outputs() {
    for initial in ["731", "3.5", "true", "null", "\"prior\"", "[1, 2]", "[\"old\" => \"value\"]"] {
        let source = format!("<?php $result = {initial}; mb_parse_str(\"key=value\", $result); echo $result[\"key\"]; ");
        clean(&source, "value");
    }
}

/// Verifies both PHP-visible output and complete retirement of native values after a query fixture.
fn clean(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
