# Issue #1821: mixed-receiver string property ownership

- [x] Refresh open issues/PRs and verify no associated PR in the issue timeline.
- [x] Preserve dirty main and create an isolated worktree from fresh origin/main.
- [x] Build and freeze a compiler from the starting source.
- [x] Compare the reported cases and runtime-name Stringable variant with PHP.
- [x] Confirm a failing regression before changing production code.
- [x] Retire adopted string source boxes after runtime-name stores.
- [x] Verify original cases, runtime dispatch, and borrowed-owner lifetime on/off.
- [x] Verify affected executable targets and emit/assemble all supported targets.
- [x] Publish a linked PR and inspect exact-head CI.

## Provenance

Issue: https://github.com/illegalstudio/elephc/issues/1821.
Starting main: `32a77fc532184b39ff32cce597f95f1a09f92d51`.
Branch: `fix/1821-mixed-string-property-ownership`.
The worktree was clean before adding regressions; the main checkout and other
worktrees remain untouched. Graft supplies code discovery and transitive callers.

## Baseline

Both original issue examples are already heap-clean on this main, on/off, with
the expected `v39` and `o39` values. They remain useful controls.
The runtime-name Stringable variant (`$o->$k = m($i)`) still leaks: 200 blocks
and 19,840 bytes after 40 writes, in both optimization modes. Its value is `o39`.
The baseline compiler and replay records are retained under the local proof
directory. A regression also checks that a borrowed Stringable remains alive
and is destroyed exactly once by its caller.
The declared-class receiver variant leaks 120 blocks/5,744 bytes at the same
loop count. The Mixed receiver variant therefore requires its own dispatch arm.

## Scope

Plain string storage retains the converted string, rather than the source Mixed
box. Both runtime-name dispatch arms now release an adopted source after the
field store. Static-name paths already retire their source and are unchanged.
The existing ownership predicate preserves scoped cleanup and caller owners.
Publishing first preserves PHP's destructor-observable assignment order. Shared
ABI helpers remain authoritative for every supported target.

A broader prototype added retirement to all stores and correctly failed the
static-name controls with duplicate release; it is excluded from the final
change. An inline call RHS also exposed the separately tracked reevaluation in
#1427. The destructor-order fixture passes a once-evaluated caller temporary to
the setter, isolating this ownership contract. An explicit `(string)` cast of a
held object exposed a separate temporary-string leak; that cast is not needed
for the borrowed-owner property-write control.

## Final evidence

- 32 focused mixed-receiver property-write tests pass on macOS ARM64, on/off.
- Six regression fixtures match exact PHP stdout and have clean heaps on Linux
  ARM64 and Linux x86_64, on/off: 24 Docker executions. x86_64 uses QEMU.
- A runtime-name Stringable probe emits and assembles for all five supported
  targets, on/off: ten checks. iOS is compilation/assembly evidence only.
- The compiler build is warning-free; the example matches PHP and has a clean
  heap in both optimization modes. Diff and assembly-comment checks pass.
- Final compiler SHA-256: `51da71cb233e4c79e2f9ec0d42315cd7471b0bc63b6a126fe12c63247957db91`.

No full local suite was run. CI supplies the full target matrix after push.

## Publication

Code commit: `1c162ce8cbe5e24e2267e940e353d1b01701411b`.
PR: https://github.com/illegalstudio/elephc/pull/1838 (draft, closes #1821).
GitHub confirmed the exact code head; no failed checks were reported at the
initial inspection. Full CI completion remains pending. This plan-only follow-up
does not change the validated compiler or regressions.
