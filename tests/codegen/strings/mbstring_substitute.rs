//! Purpose:
//! Verifies shared substitution settings and integer/string/null argument preservation.
//!
//! Called from:
//! - The codegen test binary's string module.
//!
//! Key details:
//! - Native and opaque eval paths share PHP-verified results, including binary replacements.
//! - Failed setters preserve the previous setting; named modes retain the remembered character.

use crate::support::*;

const PROGRAM: &str = r#"
function change_substitution(string|int|null $choice): string|int|bool { return mb_substitute_character($choice); }
var_dump(mb_substitute_character());
var_dump(Mb_SuBsTiTuTe_ChArAcTeR(33), change_substitution(null));
echo bin2hex(mb_scrub(chr(255))), "\n";
var_dump(change_substitution("NoNe"), mb_substitute_character());
echo bin2hex(mb_scrub("a" . chr(255) . "b")), "\n";
var_dump(change_substitution("long"), mb_substitute_character(null));
echo bin2hex(mb_scrub(chr(255))), "\n";
var_dump(change_substitution("entity"), mb_substitute_character());
echo bin2hex(mb_scrub(chr(255))), "\n";
$setter = mb_substitute_character(...);
var_dump($setter(...["substitute_character" => 0]), $setter());
echo bin2hex(mb_scrub(chr(255))), "\n";
var_dump(change_substitution(1114111), mb_substitute_character());
echo bin2hex(mb_scrub(chr(255))), "\n";
try { change_substitution("65"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { change_substitution(55296); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { change_substitution("none" . chr(0)); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
var_dump(mb_substitute_character());
"#;

const OUTPUT: &str = r#"int(63)
bool(true)
int(33)
21
bool(true)
string(4) "none"
6162
bool(true)
string(4) "long"
21
bool(true)
string(6) "entity"
21
bool(true)
int(0)
00
bool(true)
int(1114111)
f48fbfbf
mb_substitute_character(): Argument #1 ($substitute_character) must be "none", "long", "entity" or a valid codepoint
mb_substitute_character(): Argument #1 ($substitute_character) is not a valid codepoint
mb_substitute_character(): Argument #1 ($substitute_character) must be "none", "long", "entity" or a valid codepoint
int(1114111)
"#;

/// Verifies native union arguments, codepoint validation, and callable settings agree with PHP.
#[test]
fn test_mbstring_substitution_native() {
    assert_eq!(compile_and_run(&format!("<?php namespace SubstitutedText; {PROGRAM}")), OUTPUT);
}

/// Verifies opaque eval preserves integer codepoints separately from numeric string modes.
#[test]
fn test_mbstring_substitution_eval() {
    let body = PROGRAM.replace('\\', "\\\\").replace('\'', "\\'");
    assert_eq!(compile_and_run(&format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")), OUTPUT);
}

/// Verifies setters and remembered substitution characters cross the native/eval request boundary.
#[test]
fn test_mbstring_substitution_shared_request() {
    let source = r#"<?php
mb_substitute_character(33);
$source = $argc > 0 ? 'echo mb_substitute_character(), ":"; mb_substitute_character("none");' : '';
eval($source);
echo mb_substitute_character(), ":";
mb_substitute_character("long");
echo bin2hex(mb_scrub(chr(255))), ":";
mb_substitute_character(63);
echo mb_substitute_character();
"#;
    assert_eq!(compile_and_run(source), "33:none:21:63");
}
