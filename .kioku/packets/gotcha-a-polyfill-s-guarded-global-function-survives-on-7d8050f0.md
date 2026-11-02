---
id: gotcha-a-polyfill-s-guarded-global-function-survives-on-7d8050f0
type: gotcha
title: "A polyfill's guarded global function survives ONE hop from an eager file and is lost at two"
description: "An eager autoload.files entry keeps its own conditional declarations, but a file it requires loses them AND is still reported as compiler-included, so the runtime skips it and nobody declares the function; this is the shape of Symfony's mbstring polyfill"
created: 2026-09-22
sources:
  - path: src/resolver/declarations.rs
    blob: b02f739effcacdc5d84da04dd4fbddf9688117b2
  - path: src/autoload/mod.rs
    blob: 75febebd574e3204e6ffff19f9cf205e5cef8b7d
superseded_by: "bug_fix-an-eager-file-whose-subtree-lost-a-conditional-d-5bda525d"
superseded_why: "Its central claim -- that the rule never fires on Symfony -- came from an ELEPHC --check run that does not reach this path; five full builds show the rule is exactly what makes GET / render"
---

# A polyfill's guarded global function survives ONE hop from an eager file and is lost at two

## Fact

MEASURED with a four-file fixture, no Composer and no framework, `module.json` carrying only
`{"autoload":{"files":["boot/bootstrap.php"]}}`:

  ONE HOP   bootstrap.php declares `if (!function_exists('probe_upper')) { function … }`
            => AB, on both the current tree and a compiler frozen 2026-09-21.

  TWO HOPS  bootstrap.php is `return require __DIR__.'/bootstrap80.php';` behind a version guard,
            bootstrap80.php holds the guarded declarations
            => AB on the frozen compiler, `Call to undefined function probe_upper()` on the
               current tree.

The second hop puts the declarations on the include-stripping path, where
`resolver::declarations::strip_stmt` drops a conditional FunctionDecl (`None => None`) unless
`ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS=1`. The file is nevertheless reported in
`autoloaded_files`, which becomes `module.preincluded_sources`, which primes its runtime guard
cell to COMPILER_INCLUDED — and the runtime then SKIPS it
(`interpreter/include_exec.rs`, `phase=compiler_included_skip`). Nobody declares the function.
`autoload/mod.rs`'s own doc says those are files "the compiler opened AND SPLICED THEIR
DECLARATIONS IN", which is the promise being broken.

THE OBVIOUS FIX IS WRONG, MEASURED TWICE. Declining to take such a file so the runtime includes
it instead:
  - never fires on Symfony at all. Instrumented on the real `--web` build, the dropped-name map
    is EMPTY for every eager file, so no rule keyed on it can explain the mbstring failure;
  - fires on the DEAD branch elsewhere. The two-hop fixture records exactly one drop, from
    `bootstrap72.php`, the branch the version guard never takes.
Turning the gate on is not a workaround either: the widened world refuses `DeepClone.php:631`,
`Reference elements in array literals`.

DO NOT ATTRIBUTE A SYMFONY ROUTE CHANGE TO A COMPILER EDIT WITHOUT INSTRUMENTING IT. Three builds
of the same tree, `spfix` 7/8 and `incfix`/`v3` 8/8, looked like a fix and were not: the rule under
test never executed. Entry file NAMES also differ between builds (`index_<tag>.php`) and are baked
into the binary, so `asm_bytes` differing by tens of bytes proves nothing about determinism.

`tests/eager_file_polyfill_tests.rs` holds both halves: the one-hop case green, the two-hop case
`#[ignore]`d with this evidence.

## Why

It is the documented cause behind Call to undefined function twig\extension\mb_strtoupper, and the obvious fix -- declining to take the file -- is measurably wrong
