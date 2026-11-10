# Eval compile-time reduction

- [x] Measure assembly size by function and generated helper family for tiny native,
  literal-eval, opaque-eval and XML opaque-eval programs.
- [x] Identify and implement the highest-impact semantics-preserving reduction.
- [x] Add focused regression coverage for every affected supported target.
- [x] Compare baseline and changed compiler output and timing on identical inputs.
- [x] Record remaining work and retain CI budgets until target measurements justify removal.

Code Journal plan: `52bad3e9-054e-4fb5-8924-0a2c48ee4d1d`.

- [ ] Collect exact-head CI timing evidence and reassess deadline overrides.

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
Use the independent embedder and REPL results below when reassessing their budgets.
A reduction for opaque eval cannot by itself establish their budgets.

## Independent embedder and REPL probes

A separate public-API probe follows the embedder test's parser, conditional/name
resolution, constant folding, checking and EIR lowering calls. It builds the nested
literal on a 256 KiB thread and drops every deep value there, preserving the test's
stack constraint. It contains no eval or internal-object operators.

| Array nesting depth | Parse | Frontend | Type check | EIR lower | Drop |
|---|---:|---:|---:|---:|---:|
| 256 | 0.003 s | 0.032 s | 0.024 s | 1.14 s | 0.004 s |
| 512 | 0.004 s | 0.005 s | 0.085 s | 9.56 s | 0.016 s |
| 1,024 | 0.013 s | 0.009 s | 0.418 s | 94.32 s | 0.075 s |

These measurements locate the separate cost in lowering, with steep depth growth.
A debugger sample at depth 512 shows deep `PhpType` cloning in
`array_literal_type_for_ir`, including preparation of the nullable-pointer probe,
inside recursive literal inference and lowering. That identifies a candidate for
separate optimization, not a quantified attribution by itself. The bounded debugger driver
did not complete its sampling sequence; the phase measurements above came from
the successful independent run. The operator/catalog change does not address
this input by itself. A separate change now avoids the redundant copies described
below.

An isolated REPL launch comparison used the pinned baseline and changed CLI, PHP
8.5, identical bridge archives, and separate `XDG_CACHE_HOME` roots. Each submitted
`21 * 2` and returned `int(42)`. A cold host launch took 49.59 s before and 45.90 s
after; warm launches took 1.21 s and 1.05 s. The baseline runtime-object cache was
already populated by a failed link attempt, while neither host executable existed.
These are host-cache measurements, not fully cold toolchain measurements.

REPL tests deliberately create independent project/cache roots. The integrity test
also corrupts its cached executable to verify rebuilding, and the profile test
builds separate hosts. Preserve those assertions instead of replacing them with
one shared test cache. The ordinary non-XML catalog cost and linking need separate
profiling if their cold-host deadlines remain tight in CI.

Draft PR: https://github.com/illegalstudio/elephc/pull/2336, with `ci:macos`.

### Deep literal copy reduction

Indexed literal inference now rejects nested container elements before preparing
the nullable-pointer probe. It also checks whether an element can become a tagged
scalar before asking for a cloned representation, and returns already-materialized
nested literal storage directly. The type rules and emitted container layouts are
unchanged; no inference cache was added.

Repeating the public-API phase probe with these changes gives:

| Depth | EIR lower before | EIR lower after |
|---|---:|---:|
| 256 | 1.14 s | 0.353 s |
| 512 | 9.56 s | 3.31 s |
| 1,024 | 94.32 s | 36.43 s |

The depth-1,024 reduction is 61.4%. The actual small-stack regression passes in
36.46 s. Five additional focused executable tests pass: the depth-1,024 literal
pipeline (57.85 s), nullable pointer array elements, nullable pointer spreads,
nullable integer array elements and nested heterogeneous literal GC statistics.
The XML comparison above measures the operator/module changes before this
additional literal-copy reduction.


## First macOS CI evidence and review repair

Run `42895531912`, head `0139b6a5c744282fb01bd7fb83ce430ec304641c`,
reports these passing durations in the macOS codegen JUnit artifacts:

| Previously raised-budget fixture | Time |
|---|---:|
| Computed indexed incdec eval | 307.446 s |
| Native type boundaries | 189.599 s |
| Native generic property handoff | 190.904 s |
| Temporary parent handoff | 154.955 s |
| Large namespace query | 108.652 s |

The new optimized shared-operator bundle passes in 121.020 s. These observations
cover one loaded CI run; they do not establish a worst-case scheduling margin.
Keep the deadline and scheduling overrides until the review and target validation
are complete.

That run exposed two structural unit-test families still requiring inline object
arithmetic in callers. Grok review round `9baeeea3` confirmed this as one P1
coverage/CI defect, with no demonstrated runtime or heap regression. The repair
checks each caller's connection to the shared helper, checks native adapter
identity and precedence inside that helper, and keeps working-slot ownership,
cleanup and native dimension selection assertions in the original caller.
Shared add/inc tests also check both BcMath handlers before both XML conversions.

The repair passes all 15 focused target cases: both corrected structural families
and the strengthened shared-operator family on all five supported targets. The
initial CI run completed all 80 codegen jobs successfully; its aggregate remained
red because of the structural assertions corrected here.
