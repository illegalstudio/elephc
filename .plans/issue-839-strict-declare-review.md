# Issue #839 / PR #1788: strict directive review

- [x] Preserve the earlier strict-directive and inline-HTML fixes.
- [x] Rebase the existing branch onto current `origin/main`.
- [x] Reproduce dropped generic annotations and disabled bounds after bare CR comments.
- [x] Align PHPDoc line-comment termination with the lexer.
- [x] Replay focused PHPDoc, lexer, and inline-HTML regressions.
- [x] Run hygiene and Trail checks and prepare the existing PR update.
- [x] Identify the all-target unit-test timeout on the last published CI.
- [x] Reproduce and fix complete `ifdef` statement scanning.
- [x] Replay parser, diagnostic, inline-HTML and timeout-filter controls.
- [x] Prepare the single rebased commit for existing-branch publication.
- [ ] Verify CI on the published head.

## Historical strict-directive review

Reviewed head: `950e15ec54a3ff67a1f4e6af451fed9248ab4204`.
Rebased onto main `62cfeb85efb1d7bbcd149c528a8bc97d959fdbde`.
Four original patches are unchanged by range-diff; the first differs only in the
context of its codegen module registration (main added the repl module).

## Findings and changes

- `PRRT_kwDORs6r_s6r1W_t`: prescan every leading declare. Any accepted value 1
  enables strict typing for the whole physical file, including earlier ticks bodies;
  a later 0 cannot clear it. ParseModeGuard still replaces/restores per-file state.
- `PRRT_kwDORs6r_s6r1XBg`: scan a complete single statement, including elseif/else,
  do/while and catch/finally tails and nested alternative syntax. A separate real
  statement still closes the prefix in which strict_types may appear.
- Match and closure expressions followed by an operator also need scanning past their
  closing brace. Attribute groups are skipped without constructing AST; contextual
  enum classification reuses the actual parser name predicate.
- Prefix boundaries live in the cohesive declare_scan module, apart from directive
  parsing and validation. No AST variant, runtime ABI or target emitter changed.

## Verified evidence

- Warning-free compiler build after the final source edit; Rust module preambles and
  `git diff --check` checked before publication.
- Nine parser tests, including file-wide stamps, physical-file reset, complete bodies,
  attribute-bearing closure expressions, contextual enum expressions and malformed tails.
- Twenty-two strict/directive diagnostics tests; the earlier-body witness has a valid
  final call so a later strict error cannot hide a missed earlier stamp.
- Twenty-eight host codegen tests (strict typing and inline HTML), plus 17 support tests:
  final focused harness 45/45. Existing include/caller/engine-callback coercive controls pass.
- Thirty-one compiler lexer tests and 38 Magician lexer tests: 128 distinct focused
  feature/regression tests across these groups. No full local suite claimed.
- PHP-oracle witnesses for strict 1-then-0, strict 0-then-1 after an earlier body, all
  control-tail forms, HTML inside an alternative body, match/closure tails and a
  contextual enum constant expression. Static known strict mismatches are compiler
  diagnostics; PHP raises TypeError when executed.
- No Linux or iOS execution claimed. Shared frontend changes use CI's existing native
  and iOS compile/emitter coverage for the remaining targets.

Reproduce with narrow filters:

```sh
cargo test -p elephc --test parser_tests declare
cargo test -p elephc --test error_tests declare_order
cargo test -p elephc --test error_tests strict_types
cargo test -p elephc --test codegen_tests strict_types
cargo test -p elephc --test codegen_tests inline_html
cargo test -p elephc --test lexer_tests source_structure
cargo test -p elephc --test lexer_tests lfc
cargo test -p elephc-magician --lib lexer::tests
```

The earlier temporary evidence directory is no longer available. Current probes,
compiler receipts, and logs are retained inside the canonical project under
`.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/`.

## Evidence boundaries and follow-up

Three initial custom-harness CLI tests lacked runtime CARGO_BIN_EXE_elephc, so could
not launch the compiler. Correcting the harness environment resolved them; the final
full focused harness passed all 45 tests. No product change was made for that setup.

An eval fragment containing declare(strict_types=0) fails with a fragment parse error
on both reviewed baseline and candidate, while PHP prints 1. This is a separate eval
feature follow-up retained in the campaign queue, not support claimed by this fix.

## Trail interpretation

Fifteen keyword findings match descriptions of the discarded first-value/first-brace
approach against the new implementation, documentation and witnesses. The parser doc
was also refreshed to describe the full-file prescan instead of only later statements.
The actual behavior is verified by the strict 1/0, earlier-body, complete-tail and
expression-tail controls, so these are false positives. One additional match in that
count links a lexer token-span test to the personal note correcting Trail cost
provenance; the test changes no cost metadata and the new learning uses explicit zero
unmeasured costs. No immutable learning was edited.

Overlapping compound-append, DOM/test-harness and late-static-property branches were
preserved; their shared dispatcher and test-module registrations may need reconciliation.


## Bare CR annotation review

Review thread `PRRT_kwDORs6r_s6sgv2N` identifies a mismatch between the lexer,
which ends `//` and `#` comments at CR or LF, and `last_docblock`, which skips to
LF only. A real annotation after bare CR is swallowed, making template parameters
and their class bounds disappear.

An exact rebased pre-fix CLI accepts `identity(42)` despite `@template T of
Entity` and `@param T $value` after either comment kind followed by CR. The LF
and CRLF controls correctly reject that call with the generic bound diagnostic.
The regression matrix checks both comment kinds, all three line endings, comments
before and after the annotation, non-ASCII comment text, the bound, and the
parameter/return type. False annotations inside strings or line comments stay
ignored across the same line endings.


The new AST regression fails against the pre-fix branch with zero adopted template
parameters. After the one-line gap-scan repair, all 29 actual Cargo PHPDoc unit
tests pass. Three existing lexer cursor unit cases pass. All 19 original inline-HTML
codegen cases pass with optimization on and off, including the bare-CR lexer case.
Six invalid-bound CLI fixtures are rejected and six valid Entity controls are
accepted across both comment kinds and all three line endings. Compiler and
Magician dev builds are warning-free. No full local suite was run.


Current Trail check reports 20 contextual keyword matches. Existing strict-mode
scanning still preserves later enabling directives and complete control tails;
the new CR witnesses describe the failing baseline and verify its rejection after
the fix. Token-span tests do not change Trail cost metadata. These matches do not
reintroduce the discarded scanners or the annotation bypass. The immutable
learnings and overlapping worktrees are preserved.


## Ifdef boundary and CI review, 2026-10-16

Current reviewed head: ff413f6dcaa9f8ef7571ad404d1e06802bae8802.
Thread PRRT_kwDORs6r_s6tKDXZ is valid: the expression fallback swallows the next
semicolon after an ifdef block. An exact pre-fix rebuilt CLI rejects both valid
empty/else prefixes and accepts an invalid prefix containing an intervening echo.
The original parser module fails its new ifdef case, and both diagnostic cases
fail for the expected semantic reasons. Their shared root test helpers are
unchanged. A redundant Cargo test-profile root rebuild was stopped; focused
wrappers use the already built exact dev library, preserving actual test modules.

The scan now consumes Identifier + balanced then block + optional else block,
matching parse_ifdef without speculative parsing or a semicolon requirement.
Nested and malformed controls retain parser diagnostics. The selected earlier
else branch is stamped strict when the later leading directive enables it.
The parser internals page documents the compiler-extension boundary.

All four parser and two diagnostic cases pass after the repair. All 28 existing
HTML/strict codegen cases pass on/off (56 executions), excluding 17 support tests.
Three exact CLI boundary witnesses pass on/off with the intended accept/reject
status. Four native defined/undefined ifdef witnesses print the selected branch
and 7, with clean heaps in both modes. No full local suite or Linux/iOS execution
is claimed; the change is shared frontend code and fresh CI supplies the matrix.
Final compiler SHA256:146daa51c1fe81a093db39ecbb12c73dd4d4f0df24d2e9f662116dfef182af27.
All current evidence stays in internal maintenance/pr1788-ifdef/.

The prior CI's only failure was macOS non-codegen shard 3/8: 1621/1622 passed;
eval_class_name_results_release_bridge_cells_and_strings_on_all_targets timed out
at 60s on both attempts. Both Nextest profiles now allow 180s for the two exact
all-target suffix spellings, retaining the global 60s guard. Official checksum-
verified Nextest 0.9.148 accepts the matching extracted settings, selects both
positive suffixes and rejects four negative controls per profile. TOML parsing,
scoped budgets and the exact failed test name validate. This is configuration
validation; the complete fresh-head CI outcome remains pending.


Twenty Trail keyword matches are contextual. The timeout comment's two attempts
are not duplicated test registration. Historical plan entries and CR/template
regressions describe rejected baseline behavior. Current lexer/source-coordinate
docs, monotonic strict-mode preseeding and complete control/ifdef boundaries agree
with the learnings. Synthetic AST groups and parent test registrations are
preserved; generic-contract wording is unrelated to this frontend change. The
focused original-module and CLI controls verify the actual accepted/rejected
boundaries. Immutable learnings and overlapping branches remain untouched.
