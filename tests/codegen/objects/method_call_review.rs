//! Purpose:
//! Protects dynamic method argument and return contracts from the PR2326 review.
//!
//! Called from:
//! - The codegen integration test harness through `objects`.
//!
//! Key details:
//! - Each family uses distinct method names to keep dynamic signatures independent.
//! - Exceptional magic calls separately assert complete heap cleanup.

use super::*;

/// Keeps magic dispatch, PHP argument keys, Closure invocation and surplus arguments coherent.
#[test]
fn test_method_call_review_bundle() {
    assert_bundle(&[
        BundleCase {
            name: "magic_and_native_return_contract",
            source: r#"<?php
class ReviewNative { public function answer(): int { return 7; } }
class ReviewMagic { public function __call($name, $args): string { return "magic"; } }
function review_answer(mixed $o): void { var_dump([$o->answer()][0]); }
review_answer(new ReviewNative()); review_answer(new ReviewMagic());
"#,
            expected: "int(7)\nstring(5) \"magic\"\n",
        },
        BundleCase {
            name: "magic_named_keys",
            source: r#"<?php
class ReviewNamed {
    public function __call($name, $args): string { return $name . ":" . $args["z"]; }
}
function review_named(mixed $o): void {
    echo $o->missing_named(z: "yes"), "|";
    $named = ["z" => "spread"];
    echo $o->missing_named(...$named);
}
review_named(new ReviewNamed());
"#,
            expected: "missing_named:yes|missing_named:spread",
        },
        BundleCase {
            name: "magic_hidden_collector",
            source: r#"<?php
class ReviewCollector {
    public function __call($name, $args): string { return $name . ":" . func_num_args(); }
}
function review_collector(mixed $o): void { echo $o->missing_collector(1); }
review_collector(new ReviewCollector());
"#,
            expected: "missing_collector:2",
        },
        BundleCase {
            name: "magic_inferred_storage",
            source: r#"<?php
class ReviewString { public function __call($n, $a): string { return $a[0]; } }
class ReviewFloat { public function __call($n, $a): float { return $a[0]; } }
class ReviewTag {
    public function __call($n, $a): string { return gettype($a[0]) . ":" . $a[0]; }
}
function review_string(mixed $o, mixed $v): void { echo $o->missing_string($v); }
function review_float(mixed $o): void { echo $o->missing_float(2.5); }
function review_tag(mixed $o): void { echo $o->missing_tag(42); }
$rs = new ReviewString(); echo $rs->missing_string("static"), "|";
review_string($rs, "mixed"); echo "|";
$rf = new ReviewFloat(); echo $rf->missing_float(1.5), "|";
review_float($rf); echo "|";
$rt = new ReviewTag(); echo $rt->missing_tag(true), "|"; review_tag($rt);
"#,
            expected: "static|mixed|1.5|2.5|boolean:1|integer:42",
        },
        BundleCase {
            name: "closure_explicit_invoke",
            source: r#"<?php
$review_closure = function(int $x): int { return $x * 2; };
echo $review_closure->__invoke(4), "|", [$review_closure->__invoke(5)][0];
function review_invoke(mixed $f): void { echo "|", [$f->__invoke(6)][0]; }
review_invoke($review_closure);
"#,
            expected: "8|10|12",
        },
    ]);
}

/// Surplus arguments work without magic hooks, including incompatible candidate signatures.
#[test]
fn test_method_call_review_surplus_signatures() {
    let out = compile_and_run(r#"<?php
class ReviewExtraOne { public function first(int $x): int { return $x; } }
class ReviewExtraTwo {
    public function first(int $x, int $y): string { return $x . ":" . $y; }
}
function review_extra_value(): int { echo "side:"; return 2; }
function review_extra(mixed $o): void { var_dump($o->first(1, review_extra_value(), 3)); }
review_extra(new ReviewExtraOne()); review_extra(new ReviewExtraTwo());
"#);
    assert_eq!(out, "side:int(1)\nside:string(3) \"1:2\"\n");
}

/// Retires synthetic magic argument arrays and names when a selected hook throws.
#[test]
fn test_method_call_review_magic_exception_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReviewThrow {
    public function __call($name, $args): void { throw new Exception("expected"); }
}
function review_throw(mixed $o): void {
    for ($i = 0; $i < 10; $i++) {
        try { $o->missing_throw(); } catch (Exception $e) { echo "."; }
    }
}
review_throw(new ReviewThrow());
"#);
    assert!(out.success, "status={} stdout={} stderr={}", out.status, out.stdout, out.stderr);
    assert_eq!(out.stdout, "..........");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Returned containers and argument views keep independent owners after magic-call cleanup.
#[test]
fn test_method_call_review_magic_value_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReviewPayload { public string $value = "payload"; }
class ReviewEcho { public function __call($n, $a): mixed { return $a[0]; } }
class ReviewArgs { public function __call(string $n, array $a): array { return $a; } }
class ReviewReference {
    public string $value = "reference";
    public function &__call($n, $a): string { return $this->value; }
}
function review_value_owners(mixed $o, mixed $args, mixed $reference): void {
    $payload = $o->relay(new ReviewPayload());
    echo $payload->value;
    $text = str_repeat("ab", 3);
    echo "|", $o->relay($text[0]), "|", $o->relay($text), "|", $text;
    $values = $args->relay_named(k: new ReviewPayload());
    echo "|", $values["k"]->value, "|", $reference->relay_ref();
}
review_value_owners(new ReviewEcho(), new ReviewArgs(), new ReviewReference());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "payload|a|ababab|ababab|payload|reference");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
