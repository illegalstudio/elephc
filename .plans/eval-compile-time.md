# Eval compile-time reduction

- [x] Measure assembly size by function and generated helper family for tiny native,
  literal-eval, opaque-eval and XML opaque-eval programs.
- [x] Identify and implement the highest-impact semantics-preserving reduction.
- [x] Add focused regression coverage for every affected supported target.
- [x] Compare baseline and changed compiler output and timing on identical inputs.
- [ ] Record remaining work and retain CI budgets until target measurements justify removal.

Code Journal plan: `52bad3e9-054e-4fb5-8924-0a2c48ee4d1d`.

## Scope

Dynamic eval retains native declarations, synthetic builtin-class methods and generated
adapters. The runtime object already has its own cache. Measure the generated user
assembly before choosing between sharing repeated dispatch code and introducing a new
cross-module catalog ABI/cache.

Use existing `--timings` and `--emit-asm` diagnostics for compiler comparisons, with
the same PHP profile, target, optimizer and heap-debug settings. Include generated
out-of-line helpers when accounting for bytes. Separate debug/test compiler cost from
release compiler cost and cold-cache work from warm-cache work.

The embedder-stack regression does not use eval and needs independent profiling.
REPL fixtures intentionally have isolated caches and exercise cold-build and rebuild
paths. Neither should be considered fixed by a catalog optimization without evidence.

## Validation

Run compiling commands through the shared build queue. Use narrow tests for the changed
helper families, target-specific assembly checks on all five supported targets and
executable eval regressions on the host. Preserve ownership, exceptions, visibility,
by-reference arguments and class identity. Keep full-matrix timing validation with CI.

## Baseline measurements

Measured on Linux x86_64 with the debug compiler at `ad5aa69318`, PHP 8.5,
`--emit-asm --heap-debug`. Times below come from the compiler's own `--timings`,
excluding the analysis script's scan. Shared-machine timing is descriptive; emitted
byte counts provide the deterministic comparison.

| Program | Assembly bytes | Lines | EIR function markers | Compiler time |
|---|---:|---:|---:|---:|
| `echo 42` | 270,511 | 10,094 | 12 | 0.159 s |
| Literal `eval('echo 42;')` | 1,324,854 | 42,488 | 56 | 0.833 s |
| Opaque eval, no XML | 93,532,227 | 2,548,477 | 1,015 | 48.74 s |
| Opaque eval with SimpleXML | 246,101,949 | 6,842,046 | 1,489 | 116.58 s |

Accounting through existing `@fn` / `@endfn` markers gives the XML program
98,219 bytes of user main, 29,985,178 bytes of XML/DOM EIR bodies (300 markers),
82,417,985 bytes of other catalog/prelude EIR (986), 2,458,877 bytes of synthetic
EIR (202), and 131,141,690 bytes outside those markers (generated helpers and data).
Family attribution uses the function names; it is not a source-file provenance map.

XML affects unrelated catalog functions too: `DateTimeImmutable::createFromFormat`
grows from approximately 948 KB to 7.8 MB because boxed operations now contain
internal-object dispatch and its ownership scaffolding. Sharing graphs only when
both operands are already boxed removes 4,749,420 bytes (1.9%). The implementation
also admits boxed/scalar pairs through the existing `MixedBox` conversion, with
original operands and temporary boxes kept alive until the shared call finishes.

## Chosen implementation

Lower dynamic operations to ordinary EIR calls to one synthetic helper per requested
operator. Each helper uses the existing expression lowerer, including native handler
precedence and the scalar fallback. Statically known object operations, typed scalar
operations, special receiver representations and codegen-created callable wrappers
keep their existing paths. Helper names contain the compiler-only `@` separator.
PHP introspection and callable inventories exclude synthetic implementation bodies.
Increment and decrement use separate bodies that lower the original PHP update,
rather than replacing its string, bool and null behavior with arithmetic.

This reduces repeated work before EIR optimization and assembly. It does not require
a new cross-module object ABI or assume that class IDs match across programs.

Callable codegen also reuses private module snapshots between wrapper emissions.
Only each snapshot's literal pool is extended; the source EIR and metadata remain
fixed. Nested emissions take distinct snapshots. Compare the emitted assembly
before and after this pooling change on exactly the same source paths, as well as
running the executable callback/eval regressions.

## Controlled comparison

The following debug-compiler measurements used Rust 1.99.1 on Linux x86_64, with
one queued build job at a time. The machine can still have unrelated activity.
Each compiler received the same absolute source path and the flags
`--emit-asm --timings --php-version 8.5 --heap-debug`. The baseline binary was
pinned before editing, at `ad5aa69318`. GNU `as --64` assembled each output
separately. Compilation excludes assembly, linking and program execution.

Opaque source:

```php
<?php $code = $argc > 0 ? 'echo 42;' : ''; eval($code);
```

XML source:

```php
<?php
$xml = new SimpleXMLElement('<r><a>42</a></r>');
$code = $argc > 0 ? 'echo $xml->a;' : '';
eval($code);
```

| Input and compiler | Compile | Assemble | Assembly bytes | Lines | Object bytes |
|---|---:|---:|---:|---:|---:|
| Opaque, baseline | 36.23 s | 4.53 s | 93,532,279 | 2,548,477 | 26,326,808 |
| Opaque, shared graphs | 34.77 s | 5.08 s | 92,070,126 | 2,517,459 | 26,092,464 |
| Opaque, shared graphs and module reuse | 34.18 s | 4.32 s | 92,070,126 | 2,517,459 | 26,092,464 |
| XML, baseline | 145.57 s | 29.75 s | 246,102,073 | 6,842,046 | 62,850,928 |
| XML, shared graphs | 96.04 s | 14.62 s | 190,533,807 | 5,224,818 | 50,122,920 |
| XML, shared graphs and module reuse | 71.33 s | 10.28 s | 190,533,807 | 5,224,818 | 50,122,920 |

For XML, compilation plus assembly falls from 175.32 s to 81.61 s (53.5%).
Assembly shrinks 22.6%, with EIR optimization falling from 48.77 s to 24.01 s.
Code generation falls from 81.94 s to 28.23 s. These are individual local
measurements, not a CI performance guarantee or a release-compiler benchmark.

Module reuse alone preserves all 1,504 XML EIR function bodies after normalizing
local labels and string references by content. Whole-file hashes differ because
constructor thunks originate from unsorted class-map iteration. Assembly lines,
assembly bytes and object bytes match before and after module reuse.

## Validation and follow-up

- Five focused target cases validate shared calls and retained handler bodies,
  then emit assembly before and after EIR optimization: macOS ARM64, iOS device,
  iOS Simulator, Linux ARM64 and Linux x86_64.
- Existing callable descriptor, hidden clone scope and resolved-default cache
  regressions pass, including their supported-target coverage.
- The existing XML/GMP mixed operator precedence bundle passes.
- New optimized and unoptimized executable bundles cover operand order, native
  handlers, numeric/string scalar fallbacks, comparisons, overflow, exceptions,
  opaque eval, private helper visibility and heap cleanup. Both pass with the PHP
  oracle enabled and a clean heap summary (81.18 s optimized, 70.85 s unoptimized).
- The existing computed-indexed increment/decrement eval bundle passes with
  the PHP oracle enabled and clean heap diagnostics (182.11 s).
- Builtin export/regeneration, documentation audits, site compatibility and the
  builtin-to-EIR architecture gate pass with no generated documentation changes.

An exploratory PHP comparison also exposed existing Mixed arithmetic gaps:
boolean increment, array addition and array subtraction. The pinned baseline
reproduces the same output with clean heap diagnostics. Boolean increment and
missing arithmetic TypeErrors already have tasks `03217601` and `03d7a1bd`; array
union is now tracked by `a043ab27`.
Keep semantic repairs separate from this compile-time change.

Next, collect exact-head CI measurements for the five raised-budget SimpleXML
fixtures and the wider family before removing deadlines or scheduling reservations.
Profile the non-eval embedder-stack test and isolated-cache REPL tests separately.
A reduction for opaque eval cannot by itself establish their budgets.
