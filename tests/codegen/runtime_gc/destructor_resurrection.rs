//! Purpose:
//! Checks retained destructor receivers needed by callback-visible mbstring ownership paths.
//!
//! Called from:
//! - The runtime GC codegen test binary for native and opaque eval execution.
//!
//! Key details:
//! - A completed destructor runs once even when its receiver acquires another owner.
//! - Cycle collection must recompute roots after callbacks and preserve saved properties.
//! - Throwing native destructors must release their local owners before a protected caller resumes.
//! - AOT heap checks compare static-declaration overhead; eval checks observable lifetime behavior.
//!   General eval scalar-temporary growth is tracked separately from native object resurrection.

use crate::support::*;

/// Transfers new object ownership through a Mixed alias so replacement runs its destructor immediately.
#[test]
fn test_mbstring_owned_mixed_reference_assignment() {
    let source = r#"<?php
class NativeMixedAliasValue {
    public function __destruct() { echo "destroy\n"; }
}
function replace_alias(mixed $value): void {
    $alias =& $value;
    $alias = new NativeMixedAliasValue();
    echo "assigned\n";
    $alias = null;
    echo "cleared\n";
}
for ($i = 0; $i < 8; $i++) { replace_alias(null); }
echo "done\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}done\n", "assigned\ndestroy\ncleared\n".repeat(8)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Releases native destructor locals on normal and exceptional exits, including case-insensitive methods.
#[test]
fn test_mbstring_destructor_local_cleanup() {
    let source = r#"<?php
class NativeDestructorLocals {
    public function __construct(public bool $fail) {}
    public function __DESTRUCT() {
        $owned = ["key" => str_repeat("owned", 32)];
        echo count($owned), "\n";
        if ($this->fail) { throw new RuntimeException("local cleanup"); }
    }
}
function run_destructor(bool $fail): void {
    $value = new NativeDestructorLocals($fail);
    try { unset($value); echo "released\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
}
mb_strlen("");
for ($i = 0; $i < 4; $i++) { run_destructor(false); run_destructor(true); }
echo "done\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}done\n", "1\nreleased\n1\ncaught:local cleanup\n".repeat(4)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Finishes every destructor local after multiple child throws and preserves the complete exception chain.
#[test]
fn test_mbstring_destructor_local_cleanup_continues_after_child_throws() {
    let source = r#"<?php
class NativeCleanupChild {
    public function __construct(public string $name, public bool $fail) {}
    public function __destruct() {
        echo "child:", $this->name, "\n";
        if ($this->fail) { throw new RuntimeException($this->name); }
    }
}
class NativeCleanupParent {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        $first = new NativeCleanupChild("first", false);
        $second = new NativeCleanupChild("second", true);
        $third = new NativeCleanupChild("third", true);
        $fourth = new NativeCleanupChild("fourth", false);
        echo "parent\n";
        if ($this->fail) { throw new RuntimeException("parent"); }
    }
}
function run_case(bool $fail): void {
    $value = new NativeCleanupParent($fail);
    try { unset($value); }
    catch (Throwable $error) {
        echo "caught:", $error->getMessage(), "\n";
        $previous = $error->getPrevious();
        while ($previous !== null) {
            echo "previous:", $previous->getMessage(), "\n";
            $previous = $previous->getPrevious();
        }
    }
}
for ($i = 0; $i < 4; $i++) { run_case(false); run_case(true); }
echo "done\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    let one = "parent\nchild:first\nchild:second\nchild:third\nchild:fourth\ncaught:third\nprevious:second\n";
    assert_eq!(output.stdout, format!("{}done\n", format!("{one}{one}previous:parent\n").repeat(4)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Releases raw and managed Mixed references after a child throws, then continues with later locals.
#[test]
fn test_mbstring_destructor_local_cleanup_aliased_reference() {
    let source = r#"<?php
class NativeReferenceCleanupChild {
    public function __construct(public string $name, public bool $fail) {}
    public function __destruct() {
        echo "child:", $this->name, "\n";
        if ($this->fail) { throw new RuntimeException($this->name); }
    }
}
class NativeReferenceCleanupParent {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        $owned = new NativeReferenceCleanupChild("reference", true);
        $alias =& $owned;
        echo "borrow:", $alias->name, "\n";
        $after = new NativeReferenceCleanupChild("after", false);
        if ($this->fail) { throw new RuntimeException("parent"); }
    }
}
function run_case(bool $fail): void {
    $value = new NativeReferenceCleanupParent($fail);
    try { unset($value); }
    catch (Throwable $error) {
        echo "caught:", $error->getMessage(), "\n";
        $previous = $error->getPrevious();
        if ($previous !== null) { echo "previous:", $previous->getMessage(), "\n"; }
    }
}
for ($i = 0; $i < 4; $i++) { run_case(false); run_case(true); }
echo "done\n";
"#;
    let one = "borrow:reference\nchild:reference\nchild:after\ncaught:reference\n";
    let managed = source.replace("<?php", "<?php\nfunction reference_seed(mixed $value): mixed { return $value; }")
        .replace("$owned = new NativeReferenceCleanupChild", "$owned = reference_seed(null);\n$alias =& $owned;\n$owned = new NativeReferenceCleanupChild");
    for program in [source, managed.as_str()] {
        let output = compile_and_run_with_heap_debug(program);
        assert!(output.success, "{}\n{}", output.stdout, output.stderr);
        assert_eq!(output.stdout, format!("{}done\n", format!("{one}{one}previous:parent\n").repeat(4)));
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}

/// Verifies ordinary and throwing destructors preserve receivers retained in a static property.
#[test]
fn test_mbstring_destructor_resurrection_ordinary_and_throwing() {
    let body = r#"
class KeptValue {
    public function __construct(public string $text, public bool $fail) {}
    public function __destruct() {
        Keeper::$saved = $this;
        echo "drop:", $this->text, "\n";
        if ($this->fail) { throw new RuntimeException("retained"); }
    }
}
class Keeper { public static ?KeptValue $saved = null; }
function run(bool $fail): void {
    try { $value = new KeptValue("alive", $fail); unset($value); }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo Keeper::$saved->text, "\n";
    Keeper::$saved = null;
    echo "released\n";
}
mb_strlen("");
for ($i = 0; $i < 4; $i++) { run(false); run(true); }
"#;
    let expected = "drop:alive\nalive\nreleased\ndrop:alive\ncaught:retained\nalive\nreleased\n".repeat(4);
    for eval in [false, true] { assert_resurrection(body, &expected, eval); }
}

/// Preserves a callback-rescued cyclic graph and releases it later without repeating its destructor.
#[test]
fn test_mbstring_destructor_resurrection_cycles() {
    let body = r#"
class KeptCycle {
    public KeptCycle $link;
    public function __construct(public string $text, public bool $fail) { $this->link = $this; }
    public function __destruct() {
        CycleKeeper::$saved = $this;
        echo "drop:", $this->text, "\n";
        if ($this->fail) { throw new RuntimeException("retained cycle"); }
    }
}
class CycleKeeper { public static ?KeptCycle $saved = null; }
function release_saved(KeptCycle $saved): void {
    echo $saved->link->text, "\n";
    unset($saved->link);
    CycleKeeper::$saved = null;
}
function run(bool $fail): void {
    try { $value = new KeptCycle("cycle", $fail); unset($value); }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    $saved = CycleKeeper::$saved;
    if ($saved instanceof KeptCycle) { release_saved($saved); }
    else { echo "missing\n"; }
    unset($saved);
    echo "released\n";
}
mb_strlen("");
for ($i = 0; $i < 4; $i++) { run(false); run(true); }
"#;
    let expected = "drop:cycle\ncycle\nreleased\ndrop:cycle\ncaught:retained cycle\ncycle\nreleased\n".repeat(4);
    for eval in [false, true] { assert_resurrection(body, &expected, eval); }
}

/// Checks native/eval traces and compares AOT owners with unused static declaration overhead.
fn assert_resurrection(body: &str, expected: &str, eval: bool) {
    let source = if eval {
        let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
        format!("<?php $source = $argc > 0 ? '{quoted}' : ''; eval($source);")
    } else { format!("<?php {body}") };
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "eval={eval}\n{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "eval={eval}");
    if eval { return; }
    let baseline = compile_and_run_with_heap_debug(&source.replace("$i < 4", "$i < 0"));
    assert!(baseline.success, "baseline eval={eval}: {}", baseline.stderr);
    assert_eq!(live_heap(&output.stderr), live_heap(&baseline.stderr), "eval={eval}: {}", output.stderr);
}

/// Extracts live allocation counts and bytes while excluding expected allocation churn and peaks.
fn live_heap(stderr: &str) -> Vec<String> {
    stderr.lines().find(|line| line.starts_with("HEAP DEBUG: allocs="))
        .expect("heap debug report missing").split_whitespace()
        .filter(|part| part.starts_with("live_blocks=") || part.starts_with("live_bytes="))
        .map(str::to_owned).collect()
}
