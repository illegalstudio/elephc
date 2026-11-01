---
id: bug_fix-an-eager-file-whose-subtree-lost-a-conditional-d-5bda525d
type: bug_fix
title: "An eager file whose subtree lost a conditional declaration must not be reported as compiler-included"
description: "Symfony's GET / renders once the compiler declines to take an eager autoload.files entry whose include lost its guarded global functions; the runtime then includes it and declares them, and the assembly size is a bimodal oracle for the fix being in"
created: 2026-09-22
verified_by: "five Symfony --web builds, the byte-diff route gate, cargo test --lib against the 29-failure baseline"
sources:
  - path: src/autoload/mod.rs
    blob: 2cf854e181345f0e53793bedb85b641c3b30b70c
    lines: 134-175
    snip: 131fc23cf1aa
    anchor: "let mut prefix: Program = Vec::new();"
  - path: src/resolver/declarations.rs
    blob: 215151838c244eff944a6381db79634d6e49e089
    lines: 180-230
    snip: a28ae975a3fc
    anchor: "std::env::var(\"ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS\").as_deref(),"
  - path: src/resolver/engine_includes.rs
    blob: eaff8ddf36dbd143babf58eb9948a0035a8ac041
    lines: 116-145
    snip: ee3f39dce487
    anchor: "include_chain.pop();"
supersedes: "gotcha-a-polyfill-s-guarded-global-function-survives-on-7d8050f0"
---

# An eager file whose subtree lost a conditional declaration must not be reported as compiler-included

## Fact

`resolver::declarations::strip_stmt` DROPS a conditional `function` declaration in an INCLUDED
file (`None => None`) unless `ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS=1`, which is off by
default. The file is nevertheless reported in `autoloaded_files` -> `module.preincluded_sources`
-> the runtime guard cell primed to COMPILER_INCLUDED, and the runtime then SKIPS it
(`phase=compiler_included_skip`). Nobody declares the function.

FIX: an eager `autoload.files` entry is taken WHOLE or not at all. If any name dropped anywhere in
its subtree is bound nowhere in that subtree, the file is left out of the report and the runtime
includes it itself, as php does. The bound/dropped SUBTRACTION is load-bearing: a polyfill's
version guard declares the same function in both branches and only the live one is bound, so the
dead branch's drop must not condemn the file.

MEASURED, five Symfony `--web` builds, `ELEPHC_CODEGEN_JOBS=1`:

  rule absent   asm 1 426 109 362 / 1 426 109 375   GET / dies on twig\extension\mb_strtoupper
  rule present  asm 1 426 081 041 / 1 426 080 989 / 1 426 081 067   all 8 prod routes byte-identical to php -S

`asm_bytes` is bimodal with ~28 400 bytes between the bands, and the band predicts the route
outcome every time. Within a band the spread is tens of bytes, which is the ENTRY FILENAME
(`index_<tag>.php`) baked into the binary -- never read a small asm delta as nondeterminism.

Build 98-112s, container_symbols 1186 either way. Latency after: /plain 5.00ms, /greet/world
4.90ms, / 6.88ms (php -S 0.86/0.91/1.03) at load 8.2. `cargo test --release --lib` keeps the exact
29-failure baseline; codegen::objects::static_properties 37/37.

DO NOT TRUST `--check` TO INSTRUMENT A BUILD. An `eprintln` in this very rule printed NOTHING
under `elephc --check --web --ini opcache.preload=… index.php`, which I read as proof the rule
never fires on Symfony -- and published that conclusion. It was the PROBE that was wrong:
`--check` does not reach this path the way a full build does. The A/B that settled it was five
builds and their assembly sizes, not the trace.

STILL OPEN, and NOT caused by this: a two-hop bare project (`module.json` `autoload.files` ->
`bootstrap.php` -> `require bootstrap80.php` with guarded declarations) fatals with `Call to
undefined function`, with or without this rule, while a 2026-09-21 frozen compiler prints `AB`.
One hop works on both. `tests/eager_file_polyfill_tests.rs` keeps the one-hop case green and the
two-hop case `#[ignore]`d.

## Why

It is the fix for the last failing Symfony --web route, and the instrumentation that seemed to disprove it was measuring a different code path
