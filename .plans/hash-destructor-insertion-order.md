# Associative destructor insertion order

- [x] Preserve PHP oracles and old native physical-bucket-order failures.
- [x] Traverse the insertion-order chain on AArch64 and x86_64.
- [x] Preserve the successor across protected child cleanup without growing frames.
- [x] Pass assembly-comment alignment and diff hygiene.
- [x] Assemble 80 program/runtime combinations on all five supported targets.
- [x] Match PHP output and clean heaps on 48 macOS/Linux executions in both IR modes.
- [x] Pass 16 additional original-shape macOS executions.
- [x] Pass 35 focused cleanup, exception and resurrection tests per IR mode (70 total).
- [x] Run Trail checks and reconcile lexical findings in a new immutable learning.

Publication uses a dedicated fork branch, rebased onto fresh origin/main and squashed.

The previous deep-free loop scanned physical buckets. PHP-observable destructors
therefore ran in hash order, while iteration already used the insertion-order
chain. Use the stable header's head and each entry's next link for teardown too.
Capture the successor before invoking nested cleanup, since register scratch
values are not preserved across user destructors.

The former saved-capacity stack slot stores the successor. Reload capacity from
the stable header for a defensive unsigned bounds check: both -1 and out-of-range
indices end traversal. Keep frame sizes, pending Throwable storage, GC suppression,
per-entry tags and final entry/header releases unchanged.

Base used for frozen artifacts: cb407fe968e21083055914edc243131464dde234.
All source/artifact hashes and original failures remain under
.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/hash-destruction-order/.
Native cases cover strings, huge integer keys, deletion/reinsertion, growth, COW,
a deleted last entry, nested hashes and a throwing destructor. iOS device/Simulator
evidence is compilation and assembly only, never native execution.

The independent eval reference-key defect is PR1873. After this fix is validated,
combine both dependencies with the existing PR1568 review corrections and rerun
its original seven strict PHP-output/ownership fixtures without weakening them.

## Refresh on 2026-10-20

Rebased onto `487cae6abc7d98c1f16bb99042e491bb8a32e27b`. The only conflict was
module registration: retain both current-main `borrowed_numeric` and this
branch's `hash_destruction_order`. Both runtime files and the regression source
remain byte-identical to the previous published feature.

Fresh Cargo-selected CLI/library/probe/runtime-emitter artifacts are preserved
in `.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/pr1874-rebase-20261020/candidate1/`.
The focused probe passes 18 GC regression tests and 23 harness checks per EIR
mode. All 80 program/runtime pairs assemble across five targets; 48 executions
on macOS ARM64, Linux ARM64 and Linux x86_64 match complete PHP output and
report clean heaps. Those native fixtures use the original entry-point shape.
All eight PHP oracles were refreshed. Assembly comments are complete and aligned.
iOS remains compilation/assembly evidence only.
