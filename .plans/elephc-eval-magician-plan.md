# Eval and Magician: remaining frontend and AOT work

- [x] Ship dynamic eval, caller-scope synchronization, persistent declarations, literal-fragment AOT and the eval-backed REPL.
- [x] Share builtin contracts and immutable native metadata while retaining activation-local state.
- [ ] Add a maintained PHP-fragment grammar corpus shared by the compiler and Magician frontends.
- [ ] Document intentional grammar/profile differences and require a corpus case for every other PHP-visible parser disagreement.
- [ ] Reconcile dynamic-name reads, writes, probes and references with the implemented subsets and remaining tasks.
- [ ] Add access-context-aware specialization for statically known dynamic member names only when dispatch and visibility are proved.
- [ ] Expand literal AOT classification one supported semantic family at a time.
- [ ] Audit backend support and benchmark gaps against current shared contracts and the unified benchmark runner.

## Current architecture

Reviewed against release candidate `df57a9800f` on 2026-10-19. Literal AOT
reuses the compiler frontend and lowers eligible fragments through ordinary
AST-to-EIR functions. Dynamic fragments use the separate
`crates/elephc-magician` lexer/parser, EvalIR and interpreter, linked only when
needed. Magician does not depend on the full compiler crate.

`src/eval_aot/` owns literal classification/lowering. The native integration is
under `src/codegen/lower_inst/builtins/eval/` and
`src/codegen_support/runtime/eval_bridge/`. Immutable native catalogs are
shared across activations; directly accessed scalar scope names can use
selective synchronization, with a sticky full-sync fallback for indirect
access. These are shipped behavior, not future performance tasks.

Signatures, visibility, strict-PHP policy and backend support derive from
`crates/elephc-builtin-contract`, `src/builtins/` and Magician's builtin homes.
Use explicit support reasons rather than restoring manual static-only lists.
Generics in eval and the REPL are implemented. Runtime measurements belong in
`benchmark/`; keep compilation/linking outside interpreter-body measurements.

## Open semantic work

| Area | Required boundary |
|---|---|
| Shared grammar corpus | Compare pure-PHP fragment acceptance, source bytes, escapes and diagnostics; account for fragments having no outer PHP tags. |
| Profile rules | Extensions follow the physical source/profile and strict-PHP contract. Do not repeat the obsolete claim that Magician never accepts extensions. |
| Dynamic names | Cover variable variables, computed instance/static properties, nullsafe reads, and `isset`/`empty`/`unset` in read, write and reference contexts. |
| Specialization | Resolve the exact receiver and implementation; preserve lexical private/protected scope, source evaluation order, exceptions and ownership. |
| AOT expansion | Arrays/iterables, reference/by-ref values, globals/statics, exceptions, includes and declarations qualify only with an explicit model and regression coverage. Existing supported cases must not regress. |
| Builtin availability | Audit each shared contract's implemented or explicitly unsupported route on both backends. |

Tracked residuals include opaque-eval array-element reference assignment
(`94ab2dc4`), nested subscript assignment (`95e8d65f`), namespace-relative names
(`357a2224`), qualified function aliases (`7dcc1ba4`), and unknown named-argument
evaluation order (`d78cdafd`) in Code Journal. Refresh each task's source evidence
before changing code. General eval ownership work is coordinated with
[Core ownership](core-final-review-fixes.md) and the retained
[mbstring probes](mbstring-probes/README.md).

A shared dependency-light PHP syntax crate is an option only if the corpus
shows that maintaining two frontends remains too expensive. It is not a
prerequisite for shipping unrelated eval fixes.

## Guardrails and acceptance

Fallback is an intentional semantic choice, never a silent rejection or a
partial AOT execution followed by a second evaluation. Classifiers preserve
side effects and physical-source identity. Eval barriers must keep local
storage, aliases, unset state and exception ownership coherent.

For each future slice add accepted/rejected grammar cases, actual native/eval
behavior and ownership coverage, then target-aware compile/emitter coverage for
all five supported targets. The current cleanup used static checks only.
Permanent references: [eval](../docs/php/eval.md),
[eval runtime](../docs/internals/eval-runtime.md), and
[REPL](../docs/compiling/repl.md).
