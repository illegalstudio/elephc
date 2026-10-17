# PR #1823 review follow-up

- [x] Reproduce both findings against the reviewed source and PHP.
- [x] Pin the owned nested receiver and unknown-slot source over throwing stores.
- [x] Share adopted string-box cleanup across nullable and mixed receiver writes.
- [x] Add repeated caught-TypeError and successful string-store heap regressions.
- [x] Validate focused normal/exceptional cleanup and target coverage.
- [x] Prepare the verified response for the existing PR branch.

Reviewed head: `857a516f428c273f2251c4d925825ce2c56811db`.

The caught guard runs before either receiver cleanup or source-box adoption. A nested
nullable receiver needs its own unwind pin, and an unknown slot needs a source pin even
when the conservative Mixed storage model predicts a transfer. Pins retain nothing and
are detached without releasing after success, so the store's normal ownership stays intact.

String payload persistence and adopted-box retirement now live together in the shared
property-store loader. The mixed named-property dispatcher no longer repeats the latter;
the nullable receiver path consequently follows the same lifetime rule. The refined-untyped
store dispatcher also had a separate string retirement; the existing control caught its
duplicate release after centralization, so that superseded cleanup is removed too.

The two new tests reproduce 150 blocks after 30 rejected writes and 80 blocks after 40
successful string writes, while producing PHP-exact `210` and `80`. The original compound
assignment regression passes on the reviewed head. Evidence: `/private/tmp/elephc-1823-review`.

Final validation after removing both superseded string releases:
- Three nullable-property regressions pass with optimization on/off and PHP checking.
- Twenty-seven mixed-receiver controls pass in each mode, including refined-untyped strings.
- Sixteen Linux x86_64/ARM64 executions (compound, guard rejection, strings, refined-untyped
  controls; both optimization modes) are PHP-exact and heap-clean.
- The runtime and throwing program in both modes assemble for macOS ARM64, iOS device and
  iOS Simulator: nine object files, without claiming iOS execution.
- The initial shared-cleanup prototype double-released refined-untyped strings. Its control
  failure is retained; only the final v2 logs establish the passing result.
- Final `cargo build -p elephc` is warning-free; `git diff --check` passes.

## Forced Stringable follow-up

- [x] Force-build the published source and inspect fresh Stringable calls.
- [x] Keep unknown-slot source ownership explicit in EIR through store publication.
- [x] Add declared/mixed/nullable destructor-order controls and a repeated named-write control.
- [x] Replay exceptional cleanup, adjacent writes and supported-target probes.

A forced build of published `7b6733cfcc` revealed a missing normal cleanup case: a fresh
Stringable returned as Mixed leaks five blocks / 224 bytes through mixed or nullable
receivers, and its destructor does not run. Forty named mixed-receiver writes leak 120
blocks / 5,744 bytes. The declared-receiver control remains correct and heap-clean.

An unknown slot now has an explicit EIR release after the store for an owning source.
Backend arms consequently borrow/retain instead of speculating about adopting that source;
normal release happens after publication, while the existing pin owns exceptional cleanup.
This also preserves observable destructor order. The published compiler snapshot and
full PHP/heap outputs are retained in the evidence directory.

Final Stringable follow-up validation:
- Five focused tests pass (seven PHP fixtures), including literal and fresh-call typed-property
  rejections and post-publication destructors for declared/mixed/nullable receivers.
- Twenty-seven adjacent mixed-receiver controls pass after the final edit.
- Eight fixtures execute with explicit CLI optimization on/off on macOS ARM64 and both Linux
  architectures: 48 PHP-exact, heap-clean executions.
- Runtime plus normal/throwing program assembly passes on macOS/iOS device/iOS Simulator:
  fifteen object files. This is not iOS execution evidence.
- `cargo build --lib` is warning-free and `git diff --check` passes. Final evidence is isolated
  in `/private/tmp/elephc-1823-review/final-stringable`.

## Current review and issue #1834 coverage (2026-10-16)

- [x] Rebase the existing PR onto current origin/main (b56a0c8cf3), retaining both test registrations.
- [x] Reproduce the throwing-source-destructor leak on the rebased reviewed source.
- [x] Keep the receiver pin live through source destruction and detach the source before its own release.
- [x] Add two heap-debug destructor regressions and eleven issue #1834 scalar/dispatch controls.
- [x] Document property publication, source retirement, and exceptional receiver ownership in the EIR internals page.
- [x] Run the focused library suite in both IR modes: 53 Rust tests per mode, including harness controls.
- [x] Freeze and replay the final CLI, then execute the focused macOS/Linux and iOS assembly probes.
- [x] Run Trail check and prepare the single-commit update for existing fork PR #1823, closing both issues.

The eleven #1834 fixtures already pass before the new review fix on rebased 8eea99700a.
The two destructor fixtures fail there in both modes: the published text is PHP-correct,
but forty exceptions leave a receiver cell at refcount 40 and three live blocks / 128 bytes.
The split-pin change passes those fixtures and all adjacent focused controls. The existing
all-target guard control initially used the main-baseline CLI; final CLI replay is a separate gate.

Current artifacts remain inside Elephc at `.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/issue1834/within-pr1823`.
Older `/private/tmp` paths above describe historical evidence and are not used in this continuation.

Final artifact: CLI SHA-256 `37744204725ccd3855d9140eb4cc5485020f78aca63c4e3a775a95bb49d89cb3`. Final replay passes all 53 focused Rust tests in each IR mode, including the all-target control pinned to that CLI. Fifteen fixtures in both modes produce 150 program/runtime assembly checks across five targets and 90 PHP-exact, heap-clean macOS/Linux executions. iOS evidence is compilation/assembly only. Full CI remains separate.
