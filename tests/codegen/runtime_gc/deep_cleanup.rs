//! Purpose:
//! Exercises exception-safe native deep cleanup reached by mbstring argument ownership.
//!
//! Called from:
//! - The focused runtime-GC codegen test module.
//!
//! Key details:
//! - Pre-entry failures isolate destruction from PHP's retained exception argument traces.
//! - Nested callbacks must finish container storage and preserve independent inner catches.
//! - Repeated runs compare outstanding allocations after complete exception chains are released.

use crate::support::*;

/// Completes object properties and later array entries after both parent and child destructors throw.
#[test]
fn test_mbstring_deep_cleanup_nested_properties() {
    let source = r#"<?php
class CleanupLeaf {
 public function __construct(public string $name, public bool $fail = false) {}
 public function __destruct() { echo $this->name, "\n"; if ($this->fail) { throw new RuntimeException($this->name); } }
}
class CleanupOwner {
 public function __construct(public array $values) {}
 public function __destruct() { echo "owner\n"; throw new RuntimeException("owner"); }
}
function stopped_tail(): array { throw new RuntimeException("source"); }
try { mb_strlen(...[new CleanupOwner([new CleanupLeaf("child",true),new CleanupLeaf("sibling")]),new CleanupLeaf("tail")],...stopped_tail()); }
catch (Throwable $e) { echo "caught:", $e->getMessage(), "\n"; }
echo "after\n";
"#;
    assert_eq!(compile_and_run(source), "owner\nchild\nsibling\ntail\ncaught:child\nafter\n");
}

/// Preserves independent destructor catches and the complete sequence of replaced exceptions.
#[test]
fn test_mbstring_deep_cleanup_nested_catches_and_chain() {
    let source = r#"<?php
class BrokenCleanup {
 public function __construct(public string $name) {}
 public function __destruct() { echo $this->name, "\n"; throw new RuntimeException($this->name); }
}
class CaughtCleanup {
 public function __destruct() { try { $x = new BrokenCleanup("inner"); unset($x); } catch (Throwable $e) { echo "caught:", $e->getMessage(), "\n"; } }
}
function fail_tail(): array { throw new RuntimeException("source"); }
function show_chain(?Throwable $e): void { if ($e !== null) { echo "error:", $e->getMessage(), "\n"; show_chain($e->getPrevious()); } }
try { mb_strlen(...[new BrokenCleanup("first"),new CaughtCleanup(),new BrokenCleanup("last")],...fail_tail()); }
catch (Throwable $e) { show_chain($e); }
echo "after\n";
"#;
    assert_eq!(compile_and_run(source), "first\ninner\ncaught:inner\nlast\nerror:last\nerror:first\nerror:source\nafter\n");
}

/// Finishes hash values, closure captures, and following array entries after nested cleanup failures.
#[test]
fn test_mbstring_deep_cleanup_hash_and_captures() {
    let source = r#"<?php
class CapturedCleanupValue {
 public function __construct(public string $name, public bool $fail = false) {}
 public function __destruct() { echo $this->name, "\n"; if ($this->fail) { throw new RuntimeException($this->name); } }
}
function captured_cleanup(): callable {
 $first = new CapturedCleanupValue("capture-first",true);
 $last = new CapturedCleanupValue("capture-last");
 return function() use ($first,$last): void {};
}
function fail_container_tail(): array { throw new RuntimeException("source"); }
try { mb_strlen(...[["first"=>new CapturedCleanupValue("hash-first",true),"last"=>new CapturedCleanupValue("hash-last")],captured_cleanup(),new CapturedCleanupValue("tail")],...fail_container_tail()); }
catch(Throwable $e) { echo "caught:", $e->getMessage(), "\n"; }
echo "after\n";
"#;
    assert_eq!(compile_and_run(source), "hash-first\nhash-last\ncapture-first\ncapture-last\ntail\ncaught:capture-first\nafter\n");
}

/// Balances every nested container and exception owner when pre-entry cleanup repeatedly throws.
#[test]
fn test_mbstring_deep_cleanup_ownership() {
    let mut remaining = Vec::new();
    for count in [1, 24] {
        let source = format!(r#"<?php
class FailingCleanupOwner {{
    public function __construct(public array $children) {{}}
    public function __destruct() {{ throw new RuntimeException("destructor"); }}
}}
class QuietCleanupChild {{ public function __construct(public string $text) {{}} }}
function failing_cleanup_source(): array {{ throw new RuntimeException("source"); }}
for ($i = 0; $i < {count}; $i++) {{
    try {{
        mb_strlen(...[new FailingCleanupOwner([
            "label" => new QuietCleanupChild(str_repeat("x", 64)),
            "nested" => [new QuietCleanupChild(str_repeat("y", 64))]
        ])], ...failing_cleanup_source());
    }} catch (Throwable $error) {{}}
}}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        remaining.push(allocated as i64 - freed as i64);
    }
    assert_eq!(remaining[0], remaining[1], "throwing destruction retained containers or previous exceptions");
}

/// Consumes repeated exception owners without making self-links or cycles in getPrevious chains.
#[test]
fn test_mbstring_deep_cleanup_previous_intersections() {
    let source = r#"<?php
class CleanupRethrow {
    public function __construct(public Throwable $error) {}
    public function __destruct() { throw $this->error; }
}
function throw_cleanup_origin(Throwable $error): array { throw $error; }
$inner = new RuntimeException("inner");
$outer = new RuntimeException("outer", 0, $inner);
try { mb_strlen(...[new CleanupRethrow($inner)], ...throw_cleanup_origin($inner)); }
catch (Throwable $first) { echo $first->getMessage(), ":", $first->getPrevious() === null ? "none" : "cycle", "\n"; }
try { mb_strlen(...[new CleanupRethrow($inner)], ...throw_cleanup_origin($outer)); }
catch (Throwable $second) { echo $second->getMessage(), ":", $second->getPrevious() === null ? "none" : "cycle", "\n"; }
"#;
    assert_eq!(compile_and_run(source), "inner:none\ninner:none\n");
}

/// Finishes later argument destruction when a successful Stringable call throws during source cleanup.
#[test]
fn test_mbstring_deep_cleanup_after_successful_call() {
    let source = r#"<?php
class CleanupStringInput {
    public function __construct(public string $name) {}
    public function __toString(): string { return $this->name === "encoding" ? "UTF-8" : "猫"; }
    public function __destruct() {
        echo $this->name, "\n";
        if ($this->name === "subject") { throw new RuntimeException("subject"); }
    }
}
try { mb_strlen(new CleanupStringInput("subject"), new CleanupStringInput("encoding")); }
catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
echo "after\n";
"#;
    assert_eq!(compile_and_run(source), "subject\nencoding\ncaught:subject\nafter\n");
}

/// Chains compact and boxed subclass exceptions without interpreting a nullable cell as an object.
#[test]
fn test_mbstring_deep_cleanup_previous_subclass_storage() {
    let source = r#"<?php
class CleanupCustomException extends RuntimeException {
    public string $detail = "custom";
    public function __construct(string $message, int $code = 0, ?Throwable $previous = null) {
        $this->message = $message; $this->code = $code; $this->previous = $previous;
    }
}
class CleanupSubclassFailure {
    public function __construct(public Throwable $error) {}
    public function __destruct() { throw $this->error; }
}
function cleanup_subclass_source(): array { throw new RuntimeException("source"); }
function cleanup_subclass_chain(?Throwable $error): void {
    if ($error !== null) { echo $error->getMessage(), "\n"; cleanup_subclass_chain($error->getPrevious()); }
}
try { mb_strlen(...[new CleanupSubclassFailure(new PDOException("PDO"))], ...cleanup_subclass_source()); }
catch (PDOException $error) { cleanup_subclass_chain($error); }
try { mb_strlen(...[new CleanupSubclassFailure(new CleanupCustomException("custom", 0, new PDOException("inner")))], ...cleanup_subclass_source()); }
catch (Throwable $error) { cleanup_subclass_chain($error); }
"#;
    assert_eq!(compile_and_run(source), "PDO\nsource\ncustom\ninner\nsource\n");
}

/// Releases inserted nullable property boxes and the transferred previous owners on repeated failures.
#[test]
fn test_mbstring_deep_cleanup_boxed_previous_ownership() {
    let mut remaining = Vec::new();
    for count in [1, 24] {
        let source = format!(r#"<?php
class BoxedCleanupException extends RuntimeException {{
    public function __construct(string $message) {{ $this->message = $message; }}
}}
class BoxedCleanupFailure {{
    public function __destruct() {{ throw new BoxedCleanupException("destructor"); }}
}}
function boxed_cleanup_source(): array {{ throw new RuntimeException("source"); }}
for ($i = 0; $i < {count}; $i++) {{
    try {{ mb_strlen(...[new BoxedCleanupFailure()], ...boxed_cleanup_source()); }}
    catch (Throwable $error) {{}}
}}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        remaining.push(allocated as i64 - freed as i64);
    }
    assert_eq!(remaining[0], remaining[1], "nullable previous storage retained an owner per failure");
}
