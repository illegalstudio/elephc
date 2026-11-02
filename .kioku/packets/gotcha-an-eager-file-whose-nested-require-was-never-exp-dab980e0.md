---
id: gotcha-an-eager-file-whose-nested-require-was-never-exp-dab980e0
type: gotcha
title: "An eager file whose nested require was never expanded is still claimed, so nobody declares its functions"
description: "polyfill-deepclone bootstrap.php contributes no declarations at compile time yet is reported as compiler-included, so the runtime skips it and the require inside never runs; the three facets of this broken promise have three different causes"
created: 2026-09-22
verified_by: "ELEPHC_DECL_TRACE and ELEPHC_EVAL_TRACE on the compiled Symfony bin/console"
sources:
  - path: src/autoload/mod.rs
    blob: 8c7c9a3eedec9a555e3852c946023e51d6e60c9f
    lines: 134-180
    snip: f4d50fc5672a
    anchor: "let mut prefix: Program = Vec::new();"
  - path: crates/elephc-magician/src/interpreter/include_exec.rs
    blob: c7f1115fb6cccaf8017b9f06834bb4ba01cd3ea8
    lines: 195-215
    snip: 79370899fdf2
    anchor: "values: &mut impl RuntimeValueOps,"
---

# An eager file whose nested require was never expanded is still claimed, so nobody declares its functions

## Fact

`symfony/polyfill-deepclone/bootstrap.php` is an eager `autoload.files` entry:

    if (extension_loaded('deepclone')) { return; }
    if (\PHP_VERSION_ID >= 80100) { require __DIR__.'/bootstrap81.php'; }

MEASURED on the compiled `bin/console`:
  - `ELEPHC_DECL_TRACE=deepclone_to_array` is ZERO at every phase, `after autoload-run` included:
    `bootstrap81.php` was never spliced, so the declaration never entered the program.
  - `ELEPHC_EVAL_TRACE=all` shows `bootstrap.php` as `phase=compiler_included_skip` and
    `bootstrap81.php` nowhere at all: the runtime skips the parent, so the `require` inside it
    never runs either.
  - `extension_loaded('deepclone')` answers FALSE in the compiled binary, correctly, so the early
    return is NOT the cause -- that was the first guess and it is wrong.

The declaration is therefore made by NOBODY, and the call dies with
`Call to undefined function symfony\component\varexporter\deepclone_to_array()`.

WHY THE EXISTING DECLINE RULE MISSES IT. `autoload::run_collecting_included_*` declines an eager
file when a conditional declaration under it was DROPPED and bound nowhere. Here nothing was
dropped, because nothing was READ: the nested `require` was not expanded, so the subtree reports no
drops and the file keeps its claim. Compare `polyfill-mbstring/bootstrap.php`, whose
`return require ...` IS expanded and whose declarations ARE dropped -- same package shape,
different resolver outcome.

THE CRITERION CANNOT BE "WERE DECLARATIONS DROPPED". Three facets of the same promise now have
three causes: declarations dropped by the conditional-include gate (mbstring), a declaration kind
the compiled world cannot expose (traits, interfaces), and a subtree never read at all
(deepclone). A criterion stated over what the compiler ACTUALLY TOOK -- not over what it declined
-- is the only one that covers all three.

BEWARE: not claiming a file that genuinely declares nothing is not free either. Its top-level side
effects (a `define()`, a `spl_autoload_register`) were already executed by the compiled program,
and letting the runtime include it would run them twice.

## Why

It is the fourth console blocker and shows the eager-claim criterion cannot be stated over dropped declarations alone
