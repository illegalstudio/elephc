# Generics in Magician

- [x] Share generic binding rules between AOT and runtime evaluation.
- [x] Parse native generic declarations, written arguments, and typed arrays.
- [x] Instantiate functions, classes, interfaces, traits, and methods, including enum methods in persistent eval contexts.
- [x] Retain PHPDoc generic annotations and enforce bounds and type contracts.
- [x] Cover inference, explicit arguments, defaults, namespaces, persistence, errors, ownership, and strict mode with focused tests.
- [x] Document runtime behavior and add an executable example.
- [x] Match PHPDoc inheritance annotations by qualified name and clause.
- [x] Validate nullable and union array writes against their structural arms.
- [x] Preserve indexed array key/storage contracts on writes and value boundaries.
- [x] Preserve class variance while distinguishing constructor specializations.
- [x] Resolve fully written class and constructor slots before any cached instantiation.
- [x] Validate float and bool array keys using their normalized integer type.
- [x] Release failed construction receivers while suppressing their destructors.
- [x] Reproduce the second review findings and validate native cleanup.

Magician resolves type arguments from evaluated runtime values. Templates remain syntax until a call or class reference requests an instantiation, which is registered as ordinary EvalIR under its concrete name. Argument evaluation and ownership stay on the existing call paths. Generic binding rules must agree with AOT, while runtime execution continues to use boxed cells and catchable type errors.

Keep source locations and namespace imports when retaining templates. Script-cache serialization must invalidate older layouts. Native syntax obeys strict PHP mode; PHPDoc annotations remain valid in strict mode. Specialization must recurse through bodies and nested types without changing literal strings or variable names.

## Implementation

`elephc-generics` provides dependency-neutral type terms, binding conflicts, written arguments, dependent defaults, and depth limits. AOT adapts its existing type expressions through `src/generics/shared.rs`; Magician infers corresponding terms from runtime values.

The parser retains declaration prototypes and tokens marked by lexical type-parameter identity. Replay preserves imports, namespaces, literal bytes, attributes, source lines, and constructor-local shadowing. Native syntax and PHPDoc provenance remain distinct for strict mode. File-cache format 16 covers the combined generic metadata and source-spelling semantics from main and invalidates older PHPDoc inheritance rewrites, dropped scoped member annotations, and cached parse failures for comparisons after class references.

Concrete functions, class-like declarations, and methods use ordinary EvalIR execution. Function and class templates occupy separate PHP namespaces, and normal declarations reject conflicting templates. Persistent-context entry points, first-class callables, callback aliases, and call argument arrays resolve templates before normal binding. `interpreter/context_calls.rs` keeps the interpreter dispatcher small.

Runtime contracts validate bounds, variance, structural arrays, typed callbacks, local assignments, and returns. Reference parameters use dedicated reference wrappers; by-value parameters detach array boxes before COW writes. Duplicate reference targets compare normalized array keys. These paths preserve the existing boxed-cell ABI on every target.

## Validation

Focused checks cover Magician generics, lexer/parser behavior, cache serialization, callable dispatch, ownership, and reference binding. Real runtime-selected eval tests cover persistence, includes, strict PHPDoc, native caller access, the example, and heap-debug cleanup. AOT unit/error tests and focused constructor, namespace, and default regressions cover the shared binding changes.

`cargo build`, `git diff --check`, the generated builtin-doc workflow, site compatibility validation, and the target-architecture EIR boundary audit pass. Generated builtin registries and pages remain unchanged. User documentation lives in `docs/beyond-php/generics.md`, `docs/php/eval.md`, and `docs/internals/eval-runtime.md`; the executable example is `examples/eval-generics/main.php`.

## PR 1819 review follow-up

Reproduce each of the four findings on commit `7dd6c551b9` before applying the correction. Cover qualified inheritance clauses with parser and runtime tests, nullable/union local writes and rejected associative keys with native runtime tests, and class-plus-constructor variance with distinct concrete identities. Re-run focused Magician generic tests, native eval regressions, a warning-free build, and diff hygiene before publishing the correction to the draft PR.

Six added regressions fail on the original implementation and cover all four findings. The corrections match full written parent names within their own clauses, select coherent array union arms, enforce indexed keys and storage, and encode all class/constructor bindings under the original class base. Inference and variance compare the declared class slots while constructor slots keep concrete registry identities distinct. Native zero-property objects contribute type evidence; only actual empty arrays are skipped. File-cache format 13 discards parsed PHPDoc inheritance produced by the old rewrite without changing serialized shapes.

Validation passes: 46 generic parser/interpreter tests, 226 parser tests, 150 cache tests (one unrelated ignored), and 12 native eval integration tests. The native regressions cover strict PHPDoc, normalized array keys, rejected writes, structural boundaries, AOT object hierarchy, and clean heap-debug reports. `cargo build`, Rust documentation/added-text hygiene, and `git diff --check` pass.

## Second PR 1819 review

The review of `5590f4eb50` identifies two remaining regressions: a concrete class-plus-constructor spelling fails before inference creates its cache entry, and typed array writes reject float/bool keys that PHP stores as integers. Add cold-cache, inheritance, default/bound, and normalized-key regressions before correcting either path. Keep operand evaluation and native array-key diagnostics on their existing storage paths.

Both new native regressions fail on the reviewed head. Combined written lists now bind class and constructor parameters; class-only lists retain constructor inference. Float/bool keys use a temporary integer cell for validation, with balanced release and the original storage operand preserved.

Constructor rejection additionally exposed an allocated receiver leak. Construction and property initialization now release partial receivers on error and suppress their destructors, preserving metadata for escaped references. Tests cover constructor failures, native parents, by-reference receiver escape, and property-default failure cleanup.

Validation passes: 51 generic parser/interpreter tests, 20 object lifecycle tests, 33 reference tests, and 15 native eval integration tests with clean heap reports on the regression fixtures. `cargo build`, Rust documentation, diff hygiene, and task-owned temporary cleanup pass. The recompiled example prints `42|Magician|8` followed by `type checked`. A separate follow-up records pre-existing static-property and class-constant cell retention during eval context teardown.

## Third PR 1819 review

- [x] Normalize null keys to empty strings for typed array key validation.
- [x] Preserve non-array union arms and PHP string-offset write behavior.
- [x] Reproduce both findings on `2a2e5f9875` and validate ownership and focused regressions.

The latest review identifies null-key rejection and replacement of scalar union values with arrays. Cover key normalization, rejected scalar writes, string offsets, operand evaluation order, references, COW, and failed autovivification cleanup through interpreter and native runtime tests.

Both findings fail in native regressions on the reviewed head, and three added interpreter tests also fail before correction. Null keys now validate through a temporary empty string while retaining the runtime's original diagnostic operand. Local writes inspect their current receiver, autovivify only null or false, and preserve string offsets and scalar errors through reference-aware writeback. Owned operands and newly created arrays are released on rejection.

Validation passes: 54 generic parser/interpreter tests, 4 array-reference tests, 16 method reference tests, 40 operand-ownership tests, 2 append tests, and 18 native eval integration tests. The new native fixtures report clean heaps; an ordinary scalar-offset fixture also matches PHP 8.5 output and warning counts. A warning-free `cargo build`, Rust documentation checks, diff hygiene, and temporary fixture cleanup pass. A separate follow-up records the pre-existing leak from a temporary literal `foreach` receiver; bound arrays in the regression fixtures have clean heaps.

## Fourth PR 1819 review

- [x] Resolve local offset receivers after index and RHS evaluation.
- [x] Include vertical tab in PHP numeric-string offset whitespace.
- [x] Reproduce both findings on `42dd93e7ef` and validate reference/COW ownership and native parity.

The review identifies stale receiver snapshots when an operand reassigns the local, affecting both indexed and append writes. Cover changes between strings, scalars, arrays, and objects, normalize keys for the resulting receiver, and keep each operand evaluated once. Numeric string offsets must accept PHP's full whitespace set, including vertical tab.

Both reported findings fail in interpreter and native regressions on the reviewed head. `local_offset_writes` now evaluates the index and RHS once, then resolves the visible local, selects its current storage semantics, and normalizes array keys only for an array receiver. Shared cleanup covers scalar errors, string replacements, array writes, and `ArrayAccess::offsetSet`. String offset trimming explicitly includes vertical tab.

Validation passes: 57 generic parser/interpreter tests, 4 array-reference tests, 33 reference tests, 40 operand-ownership tests, 1 ArrayAccess test, 2 append tests, and 21 native eval integration tests. The updated receiver regression also verifies single index-then-value evaluation. New fixtures cover receiver type changes, noncanonical numeric keys, aliases, rejected objects, and operand exceptions with clean heaps; their output matches PHP 8.5. A warning-free `cargo build`, Rust documentation checks, and diff hygiene pass. The example remains covered by the native tests.

## Rebase onto main

- [x] Replay the five feature/review commits from `d8f084cfee` onto main `889f45203e`.
- [x] Preserve the REPL modules, parser failure cursor, and source-spelling diagnostics.
- [x] Invalidate both prior eval cache layouts with format 14 and the combined IR fingerprint.
- [x] Cover generic import spelling, first-class aliases, and persistent REPL specialization.
- [x] Validate focused generics, parser/cache, callable/ownership, and native eval/REPL regressions.
- [x] Prepare the validated branch for publication with an explicit lease against the previously reviewed head.

Specialization replay now borrows a mutable parser, as required by main's REPL failure cursor. Imported first-class generic callables resolve their base name before retaining written arguments, matching direct generic calls and preserving main's diagnostic spelling. The cache guard fingerprints the merged schema as `2604479976000790958`; format 14 prevents either old format-13 layout from being reused.

Validation passes: a warning-free `cargo build`, 59 generic parser/interpreter tests, 230 parser tests, 150 cache tests (one unrelated ignored), and 93 focused callable, reference, ownership, and recovery tests. All 27 native regressions pass: 21 generic eval tests, 3 source-spelling/undefined-function tests, and 3 persistent REPL tests. The new parser regression failed before the import correction. Range comparison preserves all three latest review-fix commits unchanged; Rust documentation checks and `git diff --check` pass.

## Fifth PR 1819 review

- [x] Preserve typed-local contracts through aliases and by-reference writes.
- [x] Accept compatible inherited generic signature returns without widening invariant slots.
- [x] Validate public members supplied by generic traits, including nested trait uses.
- [x] Retain scoped PHPDoc annotations for case-insensitive class-like keywords.
- [x] Reproduce all four findings on `b5f94385a6`, validate native storage and focused regressions, and prepare the corrections for publication.

Greptile reports four P1 findings on the rebased head. Reference contracts must follow shared storage rather than one variable name, and all write paths must validate before mutation. Signature compatibility must inspect concrete ancestors. Variance verification must inspect trait members after inherited substitutions. Parser semantic changes require a fresh file-cache version.

All four primary interpreter regressions fail on `b5f94385a6` and pass after correction. Shared reference allocation metadata now protects aliases, parameters, captures, globals, property aliases, escaped array references, and whole-value writeback without retaining cells. Allocation retirement prevents stale type constraints after address reuse. Failed typed declarations restore the prior local contract; instance and static property writes preserve shared reference storage and validate before mutation. Signature checks project class/interface ancestors; trait checks reuse method composition for aliases, overrides, and effective visibility, shielding method-local slots. File-cache format 15 invalidates dropped scoped PHPDoc annotations.

Focused validation passes: 564 parser, generic, cache, reference, trait, and ownership tests, with one existing ignored cache case, plus a warning-free build and Rust/diff hygiene. Native alias and escaped array-reference heap probes are clean. Ordinary closure captures independently retain runtime cells after context teardown; journal task `90e1d964` records that separate issue, and capture behavior checks remain separate from clean-heap assertions.

Native validation passes across focused runs: 31 generic eval cases, 3 source-spelling/undefined-function cases, 1 persistent REPL case, and 2 existing reference-lifetime regressions. The final 9-case run covers failed typed declarations, static/instance property storage, globals, native writeback, ordinary call ownership, array contracts, and property/container reference lifetimes. Heap assertions pass where enabled. Literal by-reference argument keys independently retain temporary cells, tracked in journal task `4136c2d0`; the normalization fixture uses a named local key owner. Static/capture behavior fixtures remain separate from clean-heap assertions because ordinary teardown already retains their owners.

## Sixth PR 1819 review and CI

- [x] Reproduce qualified trait-visibility rejection and unsafe alias/selection cases on `c432a43f87`.
- [x] Resolve qualified adaptations against every trait in the containing use list before variance validation.
- [x] Reproduce both Linux CI failures in the unchanged native reference-conversion regression.
- [x] Refresh persistent wrapper alias/cursor metadata before releasing a replaced value.
- [x] Validate the focused interpreter and native regressions and prepare the corrections for publication.

Greptile rates `c432a43f87` at 4/5 and flags qualified trait visibility. Three new interpreter regressions fail before correction, covering visibility-only adaptations, named aliases, nested traits, and `insteadof` selections. Variance validation now uses the same full-list qualifier resolution as ordinary class composition, including hidden methods exposed by qualified aliases.

CI run `38308009535` fails `test_mbstring_conversion_input_references` in ordinary codegen shard 6/16 on both Linux architectures. The unchanged test fails locally with the same stale nested-reference output. Whole-value scope and Cell writes now refresh the persistent wrapper's metadata from the RHS before releasing the previous payload. A new interpreter regression fails before correction and covers both write routes, cursor replacement, removed keys, and clearing metadata on scalar replacement. No expected CI output or timeout is weakened.

Latest validation passes: 206 focused Magician generic, reference, trait-adaptation and operand-ownership tests; 16 native generic, property/global, reference-lifetime and conversion regressions; a warning-free build; Rust documentation and diff hygiene. The unchanged native CI fixture now produces its original expected output. The qualified trait fixture reports a clean heap. Current-head CI and the subsequent Greptile review must be verified after publication.

## Seventh PR 1819 review

- [x] Reproduce array-element writeback losing sibling references and cursors on `2754c4f960`.
- [x] Cover both scope-variable and nested persistent `Cell` writeback paths.
- [x] Copy live receiver metadata onto each shallow clone before replacing shared storage.
- [x] Validate new and existing reference regressions, ownership, and build hygiene.
- [x] Prepare the validated fix for publication and verified review-thread resolution.

The independent October 13 review rates `2754c4f960` at 3/5 while Greptile's older summary
still rates that same SHA at 5/5. The new P1 exposes incomplete metadata propagation:
element writeback creates a shallow array clone without the context side tables, so the
whole-reference refresh erases sibling aliases and resets the cursor. Both new interpreter
tests fail before correction, including the nested `Cell` route. The native regression also
fails before correction, returning `1|x|1|x|9|0|7|1|reset` instead of PHP's
`7|y|7|y|9|0|7|8|end`. It checks sibling references, whole-array aliases, independent COW
values, and an exhausted cursor. Whole-value replacement retains its existing metadata
refresh and scalar clearing behavior. Cache format and serialized IR remain unchanged.

Focused validation passes: 122 interpreter regressions, both new native setter fixtures,
and five existing native reference/conversion/ownership fixtures. Both setter fixtures fail
on the reviewed head and match PHP after correction. Existing heap assertions pass. The
new setter fixtures keep reference-target key ownership separate: even named string keys
retain cells on the reviewed head, covered by journal task `4136c2d0`. The baseline debug
probe retains four blocks (128 bytes) across two writes; the corrected eval probe retains
two blocks (64 bytes). The metadata fix does not claim to solve that independent lifetime
issue. Rust documentation and diff hygiene pass, and the build is warning-free.

All nine feature/review commits replay cleanly onto main `32a77fc532`, with their patch
contents preserved by range-diff. Post-rebase validation repeats the warning-free build,
all 122 interpreter regressions, and all seven native fixtures successfully. Publication
uses the previously reviewed remote head as an explicit lease; subsequent CI and review
results must be attributed to the new published SHA.

## Eighth PR 1819 review

- [x] Read all comments, reviews, resolved/open threads, and expanded Greptile details on `d3770ff2fc`.
- [x] Reproduce comparisons after `instanceof` being parsed as generic arguments.
- [x] Reproduce unreachable declarations registering inside function and loop bodies.
- [x] Claim class type arguments only after a complete-list lookahead succeeds.
- [x] Hoist direct program declarations while registering nested templates at their statement position.
- [x] Invalidate cached parse errors with file-cache format 16, preserving the serialized fingerprint.
- [x] Validate parser/cache, control flow, previous findings, native ownership, and persistent REPL behavior.
- [x] Prepare the validated corrections for publication and review-thread resolution.

The complete snapshot contains seven PR comments, eight reviews, and eighteen inline threads.
Pagination is complete for every connection, including comments inside resolved threads. Greptile
rates `d3770ff2fc` at 3/5 and supplies two new P1 findings in both its summary and expanded fix
prompts. Previous reference contracts, inherited returns, trait variance, keyword casing, qualified
adaptations, and element metadata findings are acknowledged as fixed. Existing static/capture/key
and temporary-foreach ownership limitations are explicitly excluded from new findings.

Three primary unit regressions fail before correction, as do both native heap fixtures. Class
reference lookahead now probes without modifying tokens or native-syntax provenance when `<`
belongs to a PHP comparison. Complete nested generic lists still split closing shifts correctly.
Only `execute_program_statements` performs batch template registration and variance checking.
Nested statement lists register each declaration when reached, skip declarations after control
flow exits, roll back failed variance registration, and retain actual redeclaration errors.

File-cache format 16 refuses previously stored parse failures for valid class-reference comparisons;
the stored IR shape and fingerprint `2604479976000790958` remain unchanged. Focused validation
passes 575 parser, cache, generic, reference, trait-adaptation and ownership tests (one existing
ignored cache case), all ten native eval/REPL regressions, a warning-free build, PHP comparison
parity, Rust module/function documentation and diff hygiene. New native fixtures report clean
heaps. The previous element-setter behavior fixtures retain their documented separate key-owner
limitation. No unrelated ownership issue is claimed fixed by these corrections.

## Ninth PR 1819 review

- [x] Read the subsequent full review and both new P1 threads on `e50e5c5c57`.
- [x] Audit every nested statement execution caller and reproduce included forward-call failure.
- [x] Share program preparation between eval and includes, retaining strict-PHP, return and once behavior.
- [x] Reproduce unchecked variance when nested trait, parent and interface dependencies are declared later.
- [x] Recheck variance with current dependencies before cold specialization or cached class access.
- [x] Validate safe private adaptations, aliases, repeated construction and previous control-flow fixes.
- [x] Prepare the verified corrections for publication and review-thread resolution.

The automatic review of the eighth-review fix finds two missing paths. Runtime includes execute
whole programs but still used the nested source-order entry. They now share EvalProgram preparation
with direct eval, including strict-PHP checks and CLI-global seeding. An additional strict-mode
include regression exposed the missing guard before correction. Include call-site restoration,
inline HTML, return values, and once handling retain their existing replay behavior.

Nested template registration remains source ordered. A dependency declared later can only be
checked once it is registered, so class specialization revalidates the template against the current
registry, including cached or aliased class access. Unsafe trait, parent, and interface consumers
reject covariant promises before construction. Qualified private adaptations remain safe. Existing
opaque native class names keep their cached resolution if generic parsing is inapplicable.

Three primary unit regressions and both native fixtures fail on `e50e5c5c57` before correction.
Validation passes 585 focused parser/cache/generic/reference/trait/include/ownership tests with
one existing ignored cache case, all twelve native eval/include/REPL regressions, warning-free
build, and Rust/diff hygiene. The new late-dependency fixture reports a clean heap. File-cache
format 16 and fingerprint `2604479976000790958` stay unchanged because this correction changes
runtime preparation and checking, not parsing or stored IR.

## CI follow-up after the ninth review

- [x] Read the complete current-head CI result and all review/comment connections.
- [x] Identify the repeated macOS timeout in the existing five-target echo ownership fixture.
- [x] Verify its PHP source, ownership checks and assembly emission are unchanged from main.
- [x] Preserve every assertion while scheduling each supported target as its own test.
- [x] Mark six obsolete review summaries as outdated and retain the current Greptile summary.
- [x] Validate both library and CLI test binaries under the existing CI timeout.
- [x] Prepare the validated correction for publication and exact-head CI/review verification.

CI run `38440691945` on `5be0ef0378` passes 1,424 tests in macOS non-codegen shard 2/8.
The CLI copy of `echo_retires_widened_string_reads_but_preserves_concrete_borrows` reaches
both 60-second limits; its library copy passes in 46.136 seconds. The same composite fixture
passes locally in 58.311 and 50.425 seconds across both binaries, leaving little margin.
The other CI jobs pass, and the aggregate failure follows this one shard.

A shared assertion helper retains the exact opaque-eval PHP source, detached-copy releases,
concrete parameter borrows, storage-path presence checks and assembly generation. Five target
tests independently cover macOS, both iOS variants and both Linux architectures. No global
timeout, test expectation, production semantics or target coverage is changed.

All twenty inline review threads are resolved, including the eleven authored by the maintainer.
The six historical maintainer summaries are minimized with GitHub's outdated reason; their
content remains available. The current Greptile 5/5 summary remains visible.

Focused nextest validation passes all ten cases across the library and CLI binaries under the
unchanged CI profile, with serial execution matching the macOS shard. Individual cases take
9.083-11.562 seconds; all five targets generate assembly and retain every original ownership
assertion. The source and assertion body are also compared exactly with the original fixture.
The build is warning-free, and Rust documentation and diff hygiene pass. Aggregate compiler
work remains comparable; this correction changes test scheduling rather than production speed.
