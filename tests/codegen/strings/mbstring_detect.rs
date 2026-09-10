//! Purpose:
//! Checks encoding guesses and cached catalog identity in native and opaque eval programs.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - Equal list contents can intentionally produce different guesses when their identities differ.
//! - Candidate callbacks are lazy, and references remain observable after outer coercions.

use crate::support::*;

/// Wraps a PHP body for direct compilation or opaque eval while retaining binary construction calls.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Exposes defaults, strict rejection, named parameters, namespaced lookup, and callable results.
#[test]
fn test_mbstring_detect_encoding_public_calls() {
    let body = r#"
namespace EncodingGuess;
echo Mb_DeTeCt_EnCoDiNg("ASCII"), "\n";
mb_detect_order("SJIS,UTF-8");
echo mb_detect_encoding("ASCII"), "\n";
echo mb_detect_encoding(strict: true, string: "猫", encodings: "ASCII,UTF-8"), "\n";
$guess = mb_detect_encoding(...);
echo $guess(chr(233), ["ASCII", "UTF-8"], false), "\n";
echo $guess(chr(233), ["ASCII", "UTF-8"], true) === false ? "invalid\n" : "wrong\n";
echo call_user_func("mb_detect_encoding", "猫", ["UTF-8", "ASCII"], true), "\n";
echo mb_detect_encoding("", ["8bit", "BASE64"], true) === false ? "filtered\n" : "wrong\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "ASCII\nSJIS\nUTF-8\nASCII\ninvalid\nUTF-8\nfiltered\n");
    }
}

/// Preserves shared catalog identity through copies and detaches it on mutation even when contents are restored.
#[test]
fn test_mbstring_detect_encoding_catalog_identity() {
    let body = r#"
$text = "caff" . chr(168) . chr(168) . " Stra?e";
$catalog = mb_list_encodings();
$copy = $catalog;
echo mb_detect_encoding($text, $catalog), "\n";
echo mb_detect_encoding($text, $copy), "\n";
$rebuilt = [];
foreach ($catalog as $name) { $rebuilt[] = (string)$name; }
echo mb_detect_encoding($text, $rebuilt), "\n";
$copy[0] = "ASCII";
$copy[0] = "BASE64";
echo mb_detect_encoding($text, $copy), "\n";
echo mb_detect_encoding($text, $catalog), "\n";
echo mb_detect_encoding($text, mb_list_encodings()), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "GB18030\nGB18030\nSJIS\nSJIS\nGB18030\nGB18030\n");
    }
}

/// Keeps shared catalog results recognizable across native/eval globals and native callable wrappers.
#[test]
fn test_mbstring_detect_encoding_cross_backend_catalog() {
    let source = r#"<?php
$text = "caff" . chr(168) . chr(168) . " Stra?e";
$catalog = mb_list_encodings();
$source = $argc > 0 ? 'echo mb_detect_encoding($text, $catalog), "\n"; $catalog = mb_list_encodings();' : '';
eval($source);
echo mb_detect_encoding($text, $catalog), "\n";
"#;
    assert_eq!(compile_and_run(source), "GB18030\nGB18030\n");
}

/// Validates lazy Stringable candidates and leaves later callbacks untouched after a bad name.
#[test]
fn test_mbstring_detect_encoding_candidate_callbacks() {
    let body = r#"
class GuessCandidate {
    public function __construct(public string $name) {}
    public function __toString(): string { echo $this->name, "\n"; return $this->name; }
}
echo mb_detect_encoding("猫", [new GuessCandidate("ASCII"), new GuessCandidate("UTF-8")], true), "\n";
try { mb_detect_encoding("", [new GuessCandidate("bad"), new GuessCandidate("skipped")]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_detect_encoding("", []); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_detect_encoding("", "bad"); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "ASCII\nUTF-8\nUTF-8\nbad\nmb_detect_encoding(): Argument #2 ($encodings) contains invalid encoding \"bad\"\nmb_detect_encoding(): Argument #2 ($encodings) must specify at least one encoding\nmb_detect_encoding(): Argument #2 ($encodings) contains invalid encoding \"bad\"\n");
    }
}

/// Retains a cached array across cycle collection and balances repeated independent mutations.
#[test]
fn test_mbstring_detect_encoding_catalog_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 20] {
            let calls = "$copy = mb_list_encodings(); mb_detect_encoding($text, $copy); $copy[$index] = $replacement;\n".repeat(count);
            let body = format!(r#"
$text = "caff" . chr(168) . chr(168) . " Stra?e";
$index = 0;
$replacement = "ASCII";
{calls}
unset($copy);
echo mb_detect_encoding($text, mb_list_encodings());
"#);
            let output = compile_and_run_with_gc_stats(&program(&body, eval));
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, "GB18030");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "catalog result ownership grew; eval={eval}");
    }
}

/// Reads later candidate references after a callback through the detection parameter at index one.
#[test]
fn test_mbstring_detect_encoding_later_reference() {
    let body = r#"
class GuessReference {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "ASCII";
    }
}
$changer = new GuessReference();
$list = [$changer, "bad"];
$next_encoding =& $list[1];
$changer->change = function () use (&$next_encoding): void { $next_encoding = "UTF-8"; };
echo mb_detect_encoding("猫", $list, true), "\n";
echo $next_encoding, "\n";
"#;
    for eval in [false, true] {
        let body = if eval {
            body.replace("$list = [$changer, \"bad\"];\n$next_encoding =& $list[1];",
                "$next_encoding = \"bad\";\n$list = [$changer, &$next_encoding];")
        } else { body.to_string() };
        assert_eq!(compile_and_run(&program(&body, eval)), "UTF-8\nUTF-8\n");
    }
}
