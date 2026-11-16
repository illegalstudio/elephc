# Issue #1797 — strict types after a bare declare

- [x] Reproduce PHP's bare-declare prefix and strict scope boundaries.
- [x] Parse complete leading declarations while preserving callable profiles.
- [x] Expose declaration groups early without erasing ordinary lowered groups.
- [x] Replay the failed CI cases, focused parser/diagnostic tests and PHP example.

Base: `32a77fc532184b39ff32cce597f95f1a09f92d51` (fresh origin/main).
Branch: `fix/1797-bare-declare-strict-types`; isolated worktree, initially clean.
GitHub showed no dedicated fix PR; #1788 references this pre-existing gap in its
review context, while covering the separate inline-HTML/tag feature.

The original PHP program prints `01`. The exact base compiler rejects it with
the first-statement strict-types diagnostic, with optimization both on and off.
Evidence: `/private/tmp/elephc-1797/baseline.json`.

## Implementation

- The ordinary parser consumes each complete leading declare statement once.
  A scoped token-position allowance permits a strict directive only at the
  actual top-level statement being parsed, so bare/compound/nested non-strict
  declare bodies do not need a heuristic token scanner or repeated parsing.
- The main op-array's strict flag applies to earlier executable statement
  bodies. Functions, methods and closures compiled before the directive retain
  their original source profiles. A later zero does not clear an earlier one.
  These boundaries were cross-checked with local PHP.
- Groups containing declarations are exposed while file/namespace statement
  lists are built, before function discovery, then normalized idempotently
  before name-resolver symbol collection for transformed programs. Ordinary
  lowered groups retain their shape, metadata and callable/control-flow scopes.
- Bare declare bodies reject declaration-only forms, avoiding acceptance of
  invalid PHP while exposing valid declarations in compound blocks.
- Ordinary empty statements are represented as empty Synthetic groups. They
  remain no-ops and still close the leading-declare allowance.
- Documentation and a runnable example explain the scope boundary.

## Verification

After the last production edit: 23 strict-type diagnostic tests and 78 tests
matching declare pass (these filters overlap); nine parser tests and one lexer
regression pass; 11 strict-type executable tests pass with optimization on and
off; five existing declare and 39 namespace executable controls pass.
The namespace checks cover the symbol-collection normalization change.
No full local suite was run. The dev build is warning-free. Ten fixtures pass PHP-exact stdout and clean heap
checks on both Linux targets on/off (40 executions); those ten fixtures plus the
example pass on macOS on/off (22 executions). Three probes emit and assemble
on all five targets on/off (30 checks). Linux x86_64 runs under QEMU; iOS evidence
is compile/assembly only. Diff hygiene and PHP example syntax pass.

Final compiler SHA-256: `7e57a6c16467ce2bf4ceefaf70261c05c151a780ce479f6e4484fd1f7e2f8e5d`.
Full CI remains the post-publication matrix gate.

A separate #1828 investigation is retained in `/private/tmp/elephc-1828` and its
clean worktree. Its trait-enum baseline depends on #1317, already implemented
by open PRs #1827/#1674; duplicating those pending implementations was avoided.

## CI and review maintenance, 2026-10-15

Rebased on main 132b4da6f7. The frozen baseline reproduces the failed
indexed nested-append shared-bucket case with count() type errors, and adds
an undefined-key warning to associative autovivification. Indiscriminate
root Synthetic flattening also made source-map reset rows differ between
a root line and the same included line.

The selective helper preserves ordinary lowered groups. Declaration discovery
runs at the parser's file/namespace push boundary, rather than only in name
resolution: a supported literal conditional function inside declare otherwise
remained undefined even though the same ordinary conditional compiles.
The shared helper retains namespace bodies and lexical statement profiles,
and does not look inside expression-owned closures.

After the final production change, the warning-free dev build passes 20
focused codegen cases (five new group regressions, eleven strict-types controls,
three actual failed CI nested-append cases and the failed CLI source-map case),
49 parser tests and 78 declaration-related error tests. Harness support checks
also pass. The five new regressions pass with IR optimization off too.
The existing declare-strict-types example now includes an integer nested append.
It matches PHP (item:42 / item:ready / 1:7), with a clean heap.
No full local suite or additional native-target runtime matrix was run;
CI supplies the complete supported-target matrix.

An earlier prototype requested arbitrary conditional function declarations
inside declare, which the baseline also rejects. The final regression uses
the existing supported literal-conditional form and repairs its discovery
phase. Separate adjacent probes (block-constant grammar, early function
availability and dynamic mixed strict-call boundaries) are retained outside
this fix's completion claims. Failed/prototype logs remain in
/private/tmp/elephc-pr-maintenance-20261015/1839-*.

Upstream automation then advanced main to d032aaf87c (Cargo version 0.27.3 only).
The complete tracked Rust source fingerprint is identical after rebasing; the
compiler reports 0.27.3 and all five new group cases, three failed array cases,
source-map regression and PHP-equal heap-clean example pass again.
The duplicate temporary example was removed; the existing example is updated.

## Test registration and CI follow-up, 2026-10-16

The five new declaration-group fixtures passed through the isolated file harness,
but `tests/codegen/types.rs` accidentally attached the strict-types path attribute
to `declare_groups`. The ordinary parent suite consequently loaded strict tests
twice. Each module now has its own explicit path. Final validation must compile
the parent module, list the five group cases and eleven strict cases, and execute
the group cases through that registration before publication.

The fresh macOS non-codegen shard passed 1621/1622 cases. Its existing all-target
eval class-name ownership unit hit 60.019s and 60.008s on both attempts without an
assertion failure. That exact test receives the existing 180s all-target allowance
in both nextest profiles; the global 60s timeout and ownership checks are unchanged.
TOML parses and has one exact allowance in each profile.

Final registration replay compiles the actual types parent module and lists exactly
five declare_groups tests and eleven strict_types tests, without duplicate strict
registration. All five registered group tests pass on/off and all eleven strict
controls pass with the final rebuilt compiler. No production Rust source changed
in this follow-up. The nextest configuration parses with both exact allowances and
the global 60s guard preserved. Full current-head CI remains pending.
