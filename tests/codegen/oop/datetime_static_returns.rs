//! Purpose:
//! Integration tests for the late-static (`static`) returns of the DateTime/DateTimeImmutable
//! fluent methods: a subclass receiver gets its own class back, statically and at runtime.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture pins its instant and zone so the formatted output is deterministic and can be
//!   cross-checked against php-src with `ELEPHC_PHP_CHECK=1`.

use super::*;

/// Verifies every `DateTimeImmutable` fluent method returns a clone of a subclass receiver: the
/// subclass name, its own property, a function declared to return the subclass, and an unchanged
/// receiver.
#[test]
fn test_datetime_immutable_subclass_fluent_methods_return_static() {
    let out = compile_and_run(
        r#"<?php
final class Stamp extends DateTimeImmutable {
    public string $label = "default";
}
function shifted(Stamp $s): Stamp { return $s->setTimezone(new DateTimeZone("Europe/Paris")); }
function show(string $name, Stamp $r): void {
    echo $name, ": ", get_class($r), " ", $r->label, " ", $r->format("Y-m-d H:i:s.u e"), "\n";
}
$s = new Stamp("2024-01-02 03:04:05", new DateTimeZone("UTC"));
$s->label = "kept";
show("modify", $s->modify("+1 day"));
show("add", $s->add(new DateInterval("P1D")));
show("sub", $s->sub(new DateInterval("PT1H")));
show("setTimezone", shifted($s));
show("setTime", $s->setTime(10, 20, 30, 40));
show("setDate", $s->setDate(2020, 2, 29));
show("setISODate", $s->setISODate(2020, 1, 3));
show("setTimestamp", $s->setTimestamp(86400));
show("setMicrosecond", $s->setMicrosecond(123));
show("chain", $s->modify("+1 day")->setTime(1, 2)->setTimezone(new DateTimeZone("UTC")));
show("receiver", $s);
"#,
    );
    assert_eq!(
        out,
        "modify: Stamp kept 2024-01-03 03:04:05.000000 UTC\n\
         add: Stamp kept 2024-01-03 03:04:05.000000 UTC\n\
         sub: Stamp kept 2024-01-02 02:04:05.000000 UTC\n\
         setTimezone: Stamp kept 2024-01-02 04:04:05.000000 Europe/Paris\n\
         setTime: Stamp kept 2024-01-02 10:20:30.000040 UTC\n\
         setDate: Stamp kept 2020-02-29 03:04:05.000000 UTC\n\
         setISODate: Stamp kept 2020-01-01 03:04:05.000000 UTC\n\
         setTimestamp: Stamp kept 1970-01-02 00:00:00.000000 UTC\n\
         setMicrosecond: Stamp kept 2024-01-02 03:04:05.000123 UTC\n\
         chain: Stamp kept 2024-01-03 01:02:00.000000 UTC\n\
         receiver: Stamp kept 2024-01-02 03:04:05.000000 UTC\n"
    );
}

/// Verifies every mutable `DateTime` fluent method returns the subclass receiver itself, typed as
/// the subclass, with its own property intact.
#[test]
fn test_datetime_subclass_fluent_methods_return_static() {
    let out = compile_and_run(
        r#"<?php
class Moment extends DateTime {
    public int $hits = 0;
}
function bump(Moment $m): Moment { return $m->modify("+1 day"); }
function show(string $name, Moment $r, Moment $orig): void {
    echo $name, ": ", get_class($r), " ", $r->hits, " ", $r === $orig ? "same" : "other", " ", $r->format("Y-m-d H:i:s.u e"), "\n";
}
$m = new Moment("2024-01-02 03:04:05", new DateTimeZone("UTC"));
$m->hits = 7;
show("modify", bump($m), $m);
show("add", $m->add(new DateInterval("P1D")), $m);
show("sub", $m->sub(new DateInterval("PT1H")), $m);
show("setTimezone", $m->setTimezone(new DateTimeZone("UTC")), $m);
show("setTimestamp", $m->setTimestamp(86400), $m);
show("setTime", $m->setTime(10, 20, 30, 40), $m);
show("setDate", $m->setDate(2020, 2, 29), $m);
show("setISODate", $m->setISODate(2020, 1, 3), $m);
show("setMicrosecond", $m->setMicrosecond(123), $m);
"#,
    );
    assert_eq!(
        out,
        "modify: Moment 7 same 2024-01-03 03:04:05.000000 UTC\n\
         add: Moment 7 same 2024-01-04 03:04:05.000000 UTC\n\
         sub: Moment 7 same 2024-01-04 02:04:05.000000 UTC\n\
         setTimezone: Moment 7 same 2024-01-04 02:04:05.000000 UTC\n\
         setTimestamp: Moment 7 same 1970-01-02 00:00:00.000000 UTC\n\
         setTime: Moment 7 same 1970-01-02 10:20:30.000040 UTC\n\
         setDate: Moment 7 same 2020-02-29 10:20:30.000040 UTC\n\
         setISODate: Moment 7 same 2020-01-01 10:20:30.000040 UTC\n\
         setMicrosecond: Moment 7 same 2020-01-01 10:20:30.000123 UTC\n"
    );
}

/// Verifies the symfony/clock `DatePoint` shape: overrides declared `static` that return
/// `parent::…`, reached through a function declared to return the subclass.
#[test]
fn test_datetime_immutable_static_overrides_return_the_subclass() {
    let out = compile_and_run(
        r#"<?php
final class Point extends DateTimeImmutable {
    public function modify(string $modifier): static { return parent::modify($modifier); }
    public function setTimezone(DateTimeZone $timezone): static { return parent::setTimezone($timezone); }
}
function now(): Point { return (new Point("2024-01-02 03:04:05", new DateTimeZone("UTC")))->modify("+1 hour"); }
echo get_class(now()), " ", now()->format("H"), "\n";
echo get_class((new Point("2024-01-02", new DateTimeZone("UTC")))->setTimezone(new DateTimeZone("UTC"))), "\n";
"#,
    );
    assert_eq!(out, "Point 04\nPoint\n");
}

/// Verifies an override may still declare the concrete class php-src tentatively types the fluent
/// methods with, instead of repeating `static`.
#[test]
fn test_datetime_immutable_override_may_return_the_concrete_class() {
    let out = compile_and_run(
        r#"<?php
class Legacy extends DateTimeImmutable {
    public function add(DateInterval $interval): DateTimeImmutable { return parent::add($interval); }
}
class Wall extends DateTime {
    public function setTime(int $hour, int $minute, int $second = 0, int $microsecond = 0): DateTime {
        return parent::setTime($hour, $minute, $second, $microsecond);
    }
}
echo get_class((new Legacy("2024-01-02", new DateTimeZone("UTC")))->add(new DateInterval("P1D"))), "\n";
echo get_class((new Wall("2024-01-02", new DateTimeZone("UTC")))->setTime(1, 2)), "\n";
"#,
    );
    assert_eq!(out, "Legacy\nWall\n");
}

/// Verifies the immutable clone matches PHP's internal clone: the subclass `__clone()` hook runs
/// once per fluent call, a readonly subclass property is copied, and `parent::modify()` lowers
/// from a grandchild and from a trait method used by a subclass.
#[test]
fn test_datetime_immutable_fluent_clone_runs_hook_and_reaches_parent_from_grandchild_and_trait() {
    let out = compile_and_run(
        r#"<?php
class Hooked extends DateTimeImmutable {
    public static int $clones = 0;
    public int $n = 1;
    public function __clone() { self::$clones++; $this->n = 100; }
}
final class Tagged extends DateTimeImmutable {
    public readonly string $tag;
    public function __construct(string $t, string $tag) { parent::__construct($t, new DateTimeZone("UTC")); $this->tag = $tag; }
}
class Mid extends DateTimeImmutable {}
class Child extends Mid {
    public function modify(string $modifier): static { return parent::modify($modifier); }
}
class GrandChild extends Child {
    public function modify(string $modifier): static { return parent::modify($modifier . " +1 hour"); }
}
trait Bumps {
    public function bump(): static { return parent::modify("+1 day"); }
}
class Bumped extends DateTimeImmutable { use Bumps; }
$h = new Hooked("2024-01-02 03:04:05", new DateTimeZone("UTC"));
$q = $h->modify("+1 day")->setMicrosecond(5);
echo get_class($q), " ", $q->n, " ", Hooked::$clones, " ", $h->n, " ", $h->format("Y-m-d"), "\n";
$t = (new Tagged("2024-01-02 03:04:05", "x"))->add(new DateInterval("PT1H"))->setTime(5, 6);
echo get_class($t), " ", $t->tag, " ", $t->format("H:i"), "\n";
$g = new GrandChild("2024-01-02 03:04:05", new DateTimeZone("UTC"));
$gm = $g->modify("+1 day");
echo get_class($gm), " ", $gm->format("Y-m-d H:i"), " ", $g->format("Y-m-d H:i"), "\n";
$b = (new Bumped("2024-01-02", new DateTimeZone("UTC")))->bump();
echo get_class($b), " ", $b->format("Y-m-d"), "\n";
"#,
    );
    assert_eq!(
        out,
        "Hooked 100 2 1 2024-01-02\n\
         Tagged x 05:06\n\
         GrandChild 2024-01-03 04:04 2024-01-02 03:04\n\
         Bumped 2024-01-03\n"
    );
}
