---
id: gotcha-array-shift-on-a-function-static-loses-the-value-e76fcc18
type: gotcha
title: "array_shift on a function static loses the value, but not the mutation"
description: "array_shift/array_pop on a function static array return NULL instead of the element when it was written by an earlier call, silently; the array still shrinks correctly, an indexed read of the same element is correct, an initializer-seeded static is correct, and the global equivalent is correct -- an ownership fingerprint, not an array bug, and the element type is irrelevant despite a first report that said otherwise"
created: 2026-09-21
verified_by: "Reproduced beside php 8.5.10 on a compiler built before the day's edits, with four controls that all pass"
sources:
  - path: src/error_handling_prelude.rs
    blob: fb2bb4fca744d8390942c37e8e4ba47435dcda18
    lines: 700-760
    snip: 7ed40f4493a6
    anchor: "fn decl_fn_elephc_shutdown_function_state() -> Stmt {"
superseded_by: "bug_fix-an-empty-array-function-static-lost-its-data-arr-5ebc4ecb"
superseded_why: "Fixed, and my characterization in it was wrong: the mutation is NOT correct, whole arrays vanished once the push count passed the initial capacity"
---

# array_shift on a function static loses the value, but not the mutation

## Fact

`array_shift()` and `array_pop()` on a function `static` array return NULL (or an empty array)
instead of the element, when that element was written by an EARLIER call. No crash, no warning:
the program computes something else. Reproduced on a compiler built before any of the day's
edits, so it is not a regression from recent work.

    function probe(int $op): void {
        static $q = [];
        if ($op === 1) { $q[] = 'a'; $q[] = 'b'; return; }
        echo 'before=', count($q), ' ';
        $e = array_shift($q);
        echo 'returned=', var_export($e, true), ' after=', count($q), "\n";
    }
    probe(1); probe(2); probe(2);

    php:     before=2 returned='a' after=1  |  before=1 returned='b' after=0
    elephc:  before=2 returned=NULL after=1 |  before=1 returned=NULL after=0

THE MUTATION IS CORRECT — the static shrinks 2 -> 1 -> 0 exactly as php does. Only the value on
the way OUT is lost. So this is not "the builtin received a copy of the array".

The four controls all PASS, and they are what make the line sharp:

    static seeded by its INITIALIZER, `static $q = ['x','y'];`   -> 'x' then 'y', correct
    reading the same element by index, `$q[0]`                   -> correct
    push and shift within ONE call                               -> correct
    the identical shape in a `global` instead of a `static`      -> correct

So the failing combination is exactly: a value written into a static slot's array by an earlier
call, extracted by a mutating by-reference builtin. An indexed read of that same element returns
it, so the payload IS in the storage; it is lost in the extraction. That is an ownership
fingerprint — a value whose ownership is per-call where the slot is per-program — the same family
as the static-slot ownership asymmetry and static property ownership holes already recorded here.

BEWARE A NARROWER FRAMING THAT IS WRONG. The agent that first hit this reported it as "a static
array WHOSE ELEMENTS ARE ARRAYS", and reported a flat static array of scalars as correct. It is
not: `array_shift` on a static array of scalars across calls returns NULL where php returns 'a'.
The element type is irrelevant. That framing sends a fixer looking at element ownership instead
of at the slot. Its note lives in the doc comment on `decl_fn_elephc_shutdown_function_state`.

WHAT IT ALREADY COST: the shutdown-function prelude could not use a queue. It is written with a
cursor (`$callbacks[$cursor]` plus `$cursor++`, never removing an entry) purely to route around
this. Anything that queues entries in a function static is exposed the same way, and the symptom
at the call site looks like "my queue is empty" rather than like a compiler bug.

Fixtures with all four controls: `scratchpad/staticq/q.php` (six cases) and `q2.php` (the
mutation-vs-return-value discriminator, plus the initializer-seeded case).

## Why

It is a SILENT miscompile: the call site sees an empty queue, not a compiler error. It already forced the shutdown-function prelude to be written with a cursor instead of a queue, and anything that queues entries in a function static is exposed the same way.
