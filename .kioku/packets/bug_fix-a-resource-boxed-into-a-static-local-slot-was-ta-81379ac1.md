---
id: bug_fix-a-resource-boxed-into-a-static-local-slot-was-ta-81379ac1
type: bug_fix
title: "A resource boxed into a static local slot was tagged as an int, because value_php_type is already a codegen_repr"
description: "codegen_repr maps Resource to Int, which is the right representation and the wrong box tag; the store used it and is_resource() then answered false forever, which is what Symfony's ConsoleOutput stream cache tripped on"
created: 2026-09-22
verified_by: "a two-function PHP discriminator, cargo test --lib at the 29-failure baseline, and the same 13 codegen::*static* failures measured with the rule neutralised"
sources:
  - path: src/codegen/lower_inst/static_locals.rs
    blob: 77d6be808eb1048c6e115ea744a416f6b7f7e635
    lines: 47-75
    snip: 4a99b6bb776e
    anchor: "fn static_local_store_type(raw: PhpType, repr: PhpType) -> PhpType {"
  - path: src/codegen/context.rs
    blob: 30155f70691cc063b12610330588c2a926ab0c62
    lines: 455-481
    snip: 42b5a090e1c1
    anchor: "pub(super) fn value_php_type(&self, value: ValueId) -> Result<PhpType> {"
---

# A resource boxed into a static local slot was tagged as an int, because value_php_type is already a codegen_repr

## Fact

`lower_store_static_local` boxed a value into a Mixed static slot with a tag taken from
`ctx.load_value_to_result(value)?.codegen_repr()`. `codegen_repr()` maps `PhpType::Resource(_)` to
`Int`: the right REPRESENTATION — a resource payload is an i64 — and the wrong TAG, 0 instead of
9. The slot then held a stream that `is_resource()` reported as an int for the rest of its life.

THE ONE-LINE FIX DOES NOTHING. Dropping the `.codegen_repr()` changes nothing at all, because
`FunctionContext::value_php_type` (context.rs) ALREADY returns `metadata.php_type.codegen_repr()`,
and `load_value_to_result` answers it. Measured: the first attempt built clean and the probe still
failed. `raw_value_php_type` is the only accessor that keeps the resource — the same accessor
`lower_is_resource` uses, which is why `is_resource(STDOUT)` in place was always right.

NARROW ON PURPOSE. `codegen_repr()` also maps a closure `Object` to `Callable` (tag 6 -> 10) and a
`Union` to `Mixed`; the boxing WANTS those, so only `Resource` is lifted out.

DISCRIMINATOR, two functions differing in one return's type:

    static $s; if(..){return $s=STDOUT;} if(..){return $s=STDERR;} return $s=STDOUT;   // was TRUE
    static $s; if(..){return $s=STDOUT;} if(..){return $s=fopen(..)?:fopen(..);} ...   // was FALSE

The second folds to `Mixed` in `Checker::wider_type` (`_ => PhpType::Mixed`), so the slot boxes and
the tag matters; the first keeps `a == b` and never boxes.

`Symfony\Component\Console\Output\ConsoleOutput::openOutputStream()` is the failing shape verbatim,
so every compiled `bin/console` died with `The StreamOutput class needs a stream as its first
argument.`

ATTRIBUTION: `cargo test --release --test codegen_tests static` fails 13 tests WITH and WITHOUT
this rule — measured by neutralising the match and rebuilding, not by reading the test names. They
are static PROPERTY / eval / reflection tests and predate this change. `--lib` keeps its exact
29-failure baseline.

## Why

The obvious one-line fix does nothing, because the type it reads has already lost the resource
