---
id: gotcha-a-resource-loses-is-resource-once-a-function-s-i-f8593d4f
type: gotcha
title: "A resource loses is_resource() once a function's inferred return type folds to Mixed"
description: "Three returns that disagree fold to Mixed in wider_type and the returned resource comes back untagged, so Symfony's StreamOutput rejects its own stream; three returns that all type as resource are fine"
created: 2026-09-22
verified_by: "a two-function PHP discriminator measured against php 8.5.10"
sources:
  - path: src/types/checker/functions/returns.rs
    blob: fadb1b166a255afd490050b064803ff18993a120
    lines: 655-672
    snip: 714967e48bd1
    anchor: "pub(crate) fn wider_type(a: &PhpType, b: &PhpType) -> PhpType {"
---

# A resource loses is_resource() once a function's inferred return type folds to Mixed

## Fact

`is_resource()` answers false for a resource a function returned, when that function has several
`return` statements whose types DISAGREE. MEASURED against php 8.5.10, the discriminator is two
functions differing only in one return's type:

    function threeResourcesSameType(int $n) {      // all three returns type as resource
        static $s;
        if ($n === 1) { return $s = STDOUT; }
        if ($n === 2) { return $s = STDERR; }
        return $s = STDOUT;
    }                                              // php true   elephc TRUE

    function threeReturnsMixedTypes(int $n) {      // one return is resource|false
        static $s;
        if ($n === 1) { return $s = STDOUT; }
        if ($n === 2) { return $s = fopen('php://output', 'w') ?: fopen('php://stdout', 'w'); }
        return $s = STDOUT;
    }                                              // php true   elephc FALSE

`Checker::wider_type` (checker/functions/returns.rs) folds the return types pairwise and ends at
`_ => PhpType::Mixed`. Equal types keep `a == b` and never reach it, which is exactly why the first
function works. Once the inferred return is `Mixed` the resource comes back without its tag —
`runtime_value_tag(PhpType::Resource(_))` is 9 and correct, so the loss is upstream of the box.

FOUR INNOCENT SUSPECTS, each cleared by measurement, because every one of them looked right:
  - the STDOUT constant: `is_resource(STDOUT)` and `get_resource_type(STDOUT)` are correct, direct,
    through an untyped parameter, and assigned to a local;
  - the `static` slot: three returns that all type as resource keep it through the same slot;
  - union collapse: `resource only`, with no union at all, failed in an early probe;
  - the `-> I64` in `--emit-ir`: that is the resource's REPRESENTATION, and the same IR carries
    `php=resource<stream>` on every value. A one-return function with that exact signature works.
    The early probes that seemed to implicate all four collected their results into an ARRAY, and
    boxing into a heterogeneous container is its own defect — never measure this through one.

WHY IT MATTERS: `Symfony\Component\Console\Output\ConsoleOutput::openOutputStream()` is this shape
verbatim — a static cache, an early return, an `?:` fallback and a `\STDOUT` return — so every
compiled `bin/console` invocation dies with `The StreamOutput class needs a stream as its first
argument.`

## Why

It is the live blocker for Symfony's compiled bin/console, and four plausible causes around it are all innocent
