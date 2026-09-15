//! Purpose:
//! Regression tests for method-call ownership of returned/receiver objects:
//! 1. `return $this` from a fluent method must acquire the receiver, so discarding
//!    the result does not drop the refcount to zero and run the destructor while
//!    the original binding is still live (a use-after-free for classes with a
//!    destructor — it crashed with SIGBUS before the fix).
//! 2. A method-call receiver that is itself an owning temporary (a prior chained
//!    call result, or an inline `new X()`) must be released after the call, or its
//!    destructor never runs (a leak).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Liveness is asserted through a static counter incremented in the constructor
//!   and decremented in the destructor, so the tests observe exactly when objects
//!   are freed rather than depending on heap-debug output.

use crate::support::*;

/// A discarded fluent `return $this` call (class with a destructor) must not free
/// the receiver early: the object stays alive and usable, then is destructed once.
#[test]
fn test_return_this_discarded_does_not_free_receiver() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $t->set(5);
    echo "alive=" . T::$alive . ";v=" . $t->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=5\nafter=0");
}

/// A fluent chain of `return $this` calls keeps exactly one live object and
/// destructs it exactly once (the acquired intermediates are released).
#[test]
fn test_return_this_chain_balances_refcount() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $t->set(1)->set(2)->set(3);
    echo "alive=" . T::$alive . ";v=" . $t->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=3\nafter=0");
}

/// A chain whose receiver is an owning temporary that returns a NEW object each
/// step releases every intermediate, so all objects are destructed (no leak).
#[test]
fn test_chained_owning_receiver_temporaries_are_released() {
    let out = compile_and_run(
        r#"<?php
class N {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { N::$alive = N::$alive + 1; }
    public function make(): N { $o = new N(); $o->v = $this->v + 1; return $o; }
    public function __destruct() { N::$alive = N::$alive - 1; }
}
function run(): void {
    $t = new N();
    $t->make()->make();
    echo "alive=" . N::$alive . "\n";
}
run();
echo "after=" . N::$alive;
"#,
    );
    assert_eq!(out, "alive=1\nafter=0");
}

/// A chained temporary that owns a reference back to a live parent releases that
/// property after its method call, so overwriting the parent's last local can run
/// its destructor immediately instead of leaking through the discarded child.
#[test]
fn test_chained_temporary_releases_parent_owner_property() {
    let out = compile_and_run(
        r#"<?php
class ParentOwner {
    public static int $alive = 0;
    public function __construct() { ParentOwner::$alive = ParentOwner::$alive + 1; }
    public function child(): OwnedChild|false { return new OwnedChild($this); }
    public function query(): OwnedChild|false {
        $child = $this->child();
        $child->value();
        return $child;
    }
    public function __destruct() { ParentOwner::$alive = ParentOwner::$alive - 1; }
}
class OwnedChild {
    private ParentOwner $owner;
    public function __construct(ParentOwner $owner) { $this->owner = $owner; }
    public function value(): int { return 7; }
}
function run(): void {
    $owner = new ParentOwner();
    $child = $owner->query();
    echo $child->value() . ":";
    unset($child);
    $owner = null;
    echo ParentOwner::$alive;
}
run();
"#,
    );
    assert_eq!(out, "7:0");
}

/// Dynamic allocation without a constructor transfers its sole object owner into
/// the returned Mixed box, so releasing that box also releases object properties.
#[test]
fn test_dynamic_new_without_constructor_transfers_object_owner() {
    let out = compile_and_run(
        r#"<?php
class DynamicParentOwner {
    public static int $alive = 0;
    public function __construct() { DynamicParentOwner::$alive = DynamicParentOwner::$alive + 1; }
    public function __destruct() { DynamicParentOwner::$alive = DynamicParentOwner::$alive - 1; }
}
class DynamicOwnedChild {
    public ?DynamicParentOwner $owner = null;
    public function setOwner(DynamicParentOwner $owner): void { $this->owner = $owner; }
    public function __destruct() { echo "child:"; }
}
$parent = new DynamicParentOwner();
$child = __elephc_new_without_constructor("DynamicOwnedChild");
$child->setOwner($parent);
unset($child);
$parent = null;
echo DynamicParentOwner::$alive;
"#,
    );
    assert_eq!(out, "child:0");
}

/// Assigning a fluent `return $this` result keeps the (aliased) object alive while
/// the binding is in scope and frees it exactly once at scope end.
#[test]
fn test_return_this_assigned_result_is_single_owned() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $x = $t->set(9);
    echo "alive=" . T::$alive . ";v=" . $x->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=9\nafter=0");
}

/// Issue #512: an object CONSTRUCTED INSIDE a producing method and returned must survive a
/// heavy call to another class before its fields are read.
///
/// Reported on v0.26.0 as corruption or a worker crash: `$p = $this->db->find(…)` read back
/// correctly right after the return, then a syntax-highlighting call ran in between and
/// `$p->title` crashed while `$p->createdAt` came back as `4294967328` — a 32-bit-looking value
/// where a real timestamp belonged. The reporter's workaround was to rebuild the object in the
/// CALLER's frame, which is what pointed at the return boundary rather than at the producer.
///
/// Not reproducible on HEAD, so this is the regression the family
/// (#483 / #484 / #486 / #498, all closed) was missing on the read side: those were measured as
/// leaks, and this shape is what turns a leaked-then-reused block into a bad read.
///
/// Three things have to hold together, and only together:
///
/// - the producer's frame is gone by the time the fields are read — `produce()` returns and the
///   heavy call runs before `renderPaste`;
/// - the heavy call churns enough string memory to reclaim anything the producer abandoned;
/// - the whole thing repeats, so the allocator is handing back blocks it has already freed. A
///   single pass reads untouched memory and would pass whatever the ownership rules did.
///
/// Deliberately a VALUE assertion and not a heap-clean one. This fixture does leak, at one
/// block per `$p->body` / `$p->lang` handed straight to a string-returning callee — which is
/// #1018, a live and separate bug in the same family, not this shape. Pinning the reads is what
/// #512 asked for; asserting a clean heap here would only re-report #1018 under the wrong name.
#[test]
fn test_object_returned_across_a_heavy_call_keeps_its_fields() {
    let out = compile_and_run(
        r#"<?php
class Paste {
    public function __construct(
        public string $title,
        public string $lang,
        public string $body,
        public int $createdAt,
    ) {}
}

class Db {
    // The object is built INSIDE this method, from values that are not literals at the
    // construction site, and handed back across the return boundary.
    public function find(string $raw, int $now): ?Paste {
        $parts = explode("|", $raw, 4);
        if (count($parts) < 4) {
            return null;
        }
        return new Paste($parts[0], $parts[1], $parts[3], (int) $parts[2] + $now);
    }
}

class Highlighter {
    public function highlight(string $code, string $lang): string {
        $out = "";
        foreach (explode(" ", $code) as $w) {
            $out .= $lang === "php" ? htmlspecialchars($w) . " " : $w . " ";
        }
        for ($i = 0; $i < 64; $i++) {
            $out = str_replace("  ", " ", $out . " ");
        }
        return trim($out);
    }
}

class View {
    public function renderPaste(Paste $p, string $hl): string {
        return "[" . $p->title . "][" . $p->createdAt . "][" . strlen($hl) . "]";
    }
}

$db = new Db();
$hl = new Highlighter();
$view = new View();

for ($i = 0; $i < 6; $i++) {
    $p = $db->find("My Title|php|1700000000|\$x = 1; echo \$x;", 0);
    if ($p === null) {
        echo "NULL";
        continue;
    }
    $rendered = $hl->highlight($p->body, $p->lang);
    echo $view->renderPaste($p, $rendered);
}
"#,
    );
    assert_eq!(out, "[My Title][1700000000][16]".repeat(6));
}
