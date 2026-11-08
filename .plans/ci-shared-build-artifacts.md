---
title: "Shared CI build artifacts"
description: "Implementation plan for shared build artifacts across Elephc CI."
---

- [x] Inventory every build command, implicit bridge build, test selector and required check.
- [x] Define artifact identities, manifests, payloads and producer/consumer validation.
- [x] Extract build producers and separate compilation from test execution.
- [x] Coordinate PR, merge-group and main consumers through one producer graph.
- [x] Migrate PDO, PHP SDK, native package, tokenizer, differential and iOS jobs.
- [x] Reuse verified merge-group artifacts on main and expose development downloads.
- [x] Apply the same producer contract to scheduled/manual runs, nightly and release.
- [ ] Verify failure propagation, coverage, cancellation, provenance and measured savings.

Status: implementation in progress on `ci/shared-build-artifacts`.
Issue: https://github.com/illegalstudio/elephc/issues/2281
Draft PR: https://github.com/illegalstudio/elephc/pull/2282
Original baseline: main at `b62bff4ca643d7b7b6019f3d563253a8b918bb1f`.
Integrated main: `145426b7fe5cc0fd60f5d56a9d7a1c9c745280b5`; its successful queue
runs provide the updated timing baseline in `.plans/ci-artifact-validation.md`.
The user authorized implementation. Work is isolated in a ggw worktree and the PR
remains a draft until the complete rollout and validation pass.

Progress on 2026-11-08:

- Host compiler/bridge producers and checksummed manifests implemented.
- PDO and PHP SDK event ownership consolidated under the three existing top-level
  required gates. PDO variants and NTS/ZTS consumers use shared test executables.
- iOS cross-target bridges, curl docs/contracts, release frame assertions,
  tokenizer, differential and managed-native consumers migrated to artifacts.
- Exact-SHA main resolver, attempt-aware nextest bundles and development packages
  implemented; real CI integration validation remains in progress.
- First runner failures exposed container Git ownership and multi-path artifact
  layout issues, plus an irrelevant mandatory Oniguruma extra in PDO archives.
  Fixes and focused regression checks are in this branch.
- Nightly/release/benchmark now share the release producer. Consumer Cargo guards,
  full aggregate failure/cancellation checks and per-variant rerun attempts are covered.
- Main gap reconciliation records pending requests durably before dispatch, starting
  at rollout; skipped commits and intermediate HEADGREEN commits build their own SHA.
- Linux image digests are resolved once and preserved across the job graph. Actual
  Rust/native compiler, glibc and Apple SDK identities are recorded and checked.
  Final graph review caught and fixed an omitted x86_64 producer input that would
  have resolved the image twice; the regression check now covers host callers too.
- Automatic extended PR coverage follows extracted `ci-*.yml` workflows. The
  shell planner tests the actual coordinator regex against every called workflow,
  preserving the coverage that changes to the old monolithic CI file selected.
- 59 focused infrastructure tests pass, plus planner/queue cleanup, docs prebuilt
  tests, actionlint and focused Rust bridge/PDO regressions. CONTRIBUTING is updated.
- Integrated SimpleXML main, retaining its nextest overrides, XML recipe update
  and documentation source move. All 59 infrastructure tests and actionlint pass
  after integration. The final exact-head remote matrix remains in progress.
- Real CI has passed both iOS producers, specialized tool producers, all three
  host producers, all six SDK consumers and both PDO aggregate gates.
  The GMP corpus migration needs a dedicated whole-corpus nextest timeout; package
  jobs now independently verify historical main SHAs before their checkout.
  Nightly assertions follow the shared producer, tokenizer iOS restores its target
  libraries, and PDO codegen consumers preserve the original single-process model.
  Full consumer matrix validation remains active; no end-to-end green
  claim or percentage saving is made yet. Remaining: resolve any real-run failures,
  verify downloadable packages and record final comparable run measurements.

Compatibility review: live ruleset, actual queue/main runs and existing cleanup
behavior were checked before implementation. No ruleset changes are planned.

## Goal

Compile each compatible source/target/profile/feature/environment combination once,
store its outputs as immutable Actions artifacts, and make every relevant consumer
use those outputs. A failed required producer must make the build gate and final
CI gate fail. Missing or incompatible artifacts must never produce a green skip.

This includes compiler executables, Rust test executables, bridge static libraries
and generated documentation tools. Sharing only the compiler executable leaves
substantial duplicate compilation in jobs that call the compiler as a Rust library.

## Coordination

Keep `ci.yml` as the coordinating entry point for pull requests, merge groups and
pushes to main. Extract build and test families into reusable workflows. Call
each required producer once and pass its artifact identifiers to all consumers.
Calling the same reusable build workflow independently from each test workflow
would repeat the build and is not the proposed design.

Move overlapping event entry points in `pdo-live.yml` and `phpext-host-sdk.yml`
under the coordinator. Preserve existing test selection, labels, platform coverage,
service setup, timeouts and retries. Keep the three existing required aggregate
checks as ordinary top-level jobs in the coordinator with their exact current
names. Reusable workflows supply results to these jobs; their nested check names
are not used as replacements for the required aggregate names. Each aggregate
runs with `if: always()` and explicitly rejects failed/canceled prerequisites,
missing outputs and unexpected skips. The new `Build` check is a prerequisite of
`Build & Test`, so adding it does not require changing branch protection.

Remove superseded independent PR/merge-group triggers in the same transition
that introduces their top-level replacement gates. Preserve standalone schedule
and manual wrappers. A single PR or merge-group run must not have old and new
producers emitting competing checks with the same required name. Confirm the
reported names through the Checks API on the refactoring PR before queue entry.

Keep scheduled and manual entry points as wrappers around the same coordinator
and build contract. A PDO schedule requests its required host and feature profiles;
the Simulator HTTPS schedule requests its host compiler and Simulator dependencies.
Avoid unnecessary whole-matrix builds for a narrowly selected manual suite.

Consumers depend on their required producers. Linux tests can start when their
Linux artifacts are ready, without waiting for macOS. An aggregate `Build` check
requires every planned producer to succeed. The final existing CI gates require
both build success and every selected test family, including correct handling of
failure, cancellation and unexpected skips.

Repository checks without compiler dependencies can run in parallel with builds.

## Merge queue compatibility contract

The active `main` ruleset is ID `22906757`, not a legacy branch-protection record:
https://github.com/illegalstudio/elephc/rules/22906757

Required contexts inspected live:

- `Build & Test`
- `PDO core live suites (PostgreSQL 16 + MySQL 8.4)`
- `PDO optional-driver live suites`

The queue uses `MERGE`, `HEADGREEN`, at most two entries building concurrently,
at most five entries merging together and a 240-minute check timeout. This
refactor preserves those settings. Queue concurrency counts merge-group entries,
not the platform build jobs inside one entry.

Keep `merge_group: types: [checks_requested]`. Resolve sources from the event's
immutable integration SHA. A green PR head or PR test merge does not replace
verification of the merge-group SHA, which includes the current base and earlier
queue entries. Keep all extended tiers enabled for merge-group events.

The following repository integration points must change together:

- `plan_tiers.sh`: it currently queries successful runs separately by workflow
  file, event and SHA. A called PDO workflow belongs to the coordinator run;
  querying `pdo-live.yml` after consolidation no longer finds that proof. Return
  explicit producer run/attempt IDs, artifact availability and per-suite test
  evidence, instead of carrying the old two-workflow lookup into the new graph.
- `cancel_rebuilt_queue_runs.sh`: cancellation belongs to the top-level entry.
  Cancel older groups of the same PR, preserve newer groups and other PRs, and
  never run a duplicate child cleanup against its own parent run.
- `merge-queue-cleanup.yml` and `cancel_superseded_queue_runs.sh`: update the
  workflow inventory when event ownership moves. Keep exact-head runs, later
  entries and diverged groups; cancel only obsolete strict-ancestor groups.
- Called workflows: pass SHA, selected tiers, artifact IDs and required settings
  explicitly. Caller `env` does not propagate automatically. Give artifact readers
  the needed `actions: read`; called workflows cannot elevate parent permissions.
  Own concurrency at the coordinator, avoiding child groups that cancel parents
  or siblings. Keep PR/group cancellation separate from main artifact retention.
- Nightly/release selection: preserve the exact `Build & Test` check name they
  currently query. Publication must also require the appropriate test evidence
  and compatible artifacts, not just successful artifact production.

Actual reuse evidence: queue CI run `42439918696`, queue PDO run `42439918652`
and current main all identify SHA `b62bff4ca643d7b7b6019f3d563253a8b918bb1f`.
Both queue runs completed successfully; six nextest artifacts for the three
hosts exist on the CI run with that same SHA. Their current retention is one day.
Keep large test-archive retention separate from the proposed 14-day development
package retention.

`HEADGREEN` allows an integrated group to pass even while earlier groups are
unfinished. Main may therefore advance over multiple commits in one push. The
passing tip's artifacts and verdict describe that tip only. Do not attach those
artifacts or copy its green verdict to intermediate commits. Any promised
per-commit backfill must resolve/build/validate its own SHA and stay outside the
queue's required path; it must not keep obsolete queue runs alive.

## Artifact contract

The identity includes full source SHA, effective source-content fingerprint for
version stamping, host and target triples, Rust toolchain, Cargo profile, exact
feature set, dependency lockfile, build scripts/configuration and relevant native
library/SDK/compiler ABI inputs. Record immutable container image identity for
container builds and actual SDK/toolchain identity for Apple targets.

Record producer repository, workflow identity/content, event, run ID, run attempt,
artifact IDs, schema version, file list and checksums. Select an explicit producer
run and validate its manifest; do not select an artifact merely by a branch name,
short SHA, name prefix or a latest-success query. Reuse across trust boundaries
must not allow a PR artifact to supply privileged release/signing jobs.

Use separate payloads so a CLI-only consumer does not download a large test bundle:

| Payload | Contents | Consumers |
| --- | --- | --- |
| Host compiler | `elephc`, required notices and build manifest | CLI-driven checks and development packages |
| Host bridges | Exact selected `libelephc_*.a` variants | Codegen, native and PHP host checks |
| Rust tests | Relocatable nextest archives, metadata and pinned runner | Shards, selected ignored/live tests and SDK tests |
| iOS bridges | Static libraries per device/Simulator target | iOS compile/link and live HTTPS checks |
| Auxiliary tools | Documentation exporter and other selected generated tools | Documentation/contract checks |
| Release profile | Compiler, bridge archives and profile-specific test executables | Benchmark, release-mode checks and release packaging |

Retain only required outputs, not an indiscriminate upload of the whole `target/`.
Keep dependency caching as a build accelerator; it is not proof of a valid artifact.
Preserve executable modes through archive packaging and validate download integrity
with bounded retries, extending the existing download action where appropriate.

## Variants and remaining compilation

Default and curl-enabled builds remain distinct where their catalog/test behavior
differs. Do not enable all features globally to make sharing easier. Build each
requested PDO feature profile once, including the root test binary's feature set
and any environment-selected native-link variant. Share the libpq profile between
compatible MySQL/PostgreSQL consumers rather than building it in both jobs.

The six PHP SDK consumers keep their NTS/ZTS runtimes and real-host checks.
Share Rust executables between compatible SDK consumers only after verifying
that no compile-time SDK-dependent input differs. PHP SDKs themselves can be
non-relocatable and must retain their own build/cache contract.

Build the macOS host compiler once for all ten current iOS compile/link consumers.
Build the required Rust bridges once per iOS target and configuration. Linking
the actual PHP/C fixtures and exercising the real SDK remain test operations.

Doctests and tests whose purpose is to compile recipes, libraries, PHP extensions
or user programs still perform that work. Managed-native smoke must keep its
deliberately empty native cache, while consuming the shared compiler. Do not
weaken cold-build coverage by preloading the product under test.

In artifact-consumption mode, unexpected lazy Rust workspace rebuilds must fail
with an actionable missing-variant error. Preserve explicit local developer
build paths outside CI. Add a gate auditing consumer workflows/scripts for
unplanned compiler builds and record observed Rust compilation during validation.

## Main and user downloads

On a push to main, first look for a trusted successful merge-group run for the
exact same full SHA with compatible complete artifacts and successful evidence
for all required test families. Artifact existence or a green `Build` check alone
does not authorize skipping tests. During migration, a successful old `ci.yml`
run does not prove PDO: its separate queue run must also have passed. For the new
coordinator, require the three aggregate verdicts and their selected-tier evidence.
Reuse that verified set without rebuilding or rerunning already-verified suites.
Independently selected main checks still run using their corresponding profile.

The queue may merge when required checks pass while unrelated jobs are still
running. A resolver must distinguish a finished required gate from whole-workflow
completion and avoid launching a duplicate build solely because the run still has
optional jobs. Reuse only fully uploaded, validated artifacts whose required
producer and test gates succeeded, or wait/retry within a bounded policy.

If no suitable run exists, artifacts have expired, or the configuration differs,
build the required profiles normally and explicitly select the tests needed to
validate those outputs. Do not combine a newly rebuilt artifact with an inherited
`run=false`/`extended=false` that silently skips its required validation.
Do not reuse a PR head or synthetic merge SHA that differs from main's actual SHA.

Keep the benchmark release build selected only for main, as today. The ordinary
queue produces debug host artifacts and does not have an equivalent macOS release
artifact to promote. Build that main-only profile once unless an exact compatible
release artifact independently exists; do not move its cost into the queue just
to make main show no builds.

Produce `elephc-dev-<target>-<sha>.tar.gz` packages from verified debug outputs,
containing the compiler, required bridge variants, notices and provenance. Expose
them on the main Actions run with a summary link and a proposed 14-day retention.
They are development artifacts, not tags or GitHub Releases. Check package
completeness and a representative compile/link from an empty installation folder.
Document actual Linux compatibility and runtime prerequisites of CI-built binaries.

The existing main concurrency policy cancels superseded runs. To satisfy the
request for an artifact for every arriving main commit, keep artifact production
and promotion keyed by full SHA, so a newer main push cannot cancel an earlier
commit before its development packages have been retained. PR and rebuilt queue
work may still be canceled when superseded. Do not serialize every SHA behind a
single rolling concurrency group that can replace pending commits.

Account explicitly for skip directives: the current release preparation pushes a
version-bump commit with `[skip ci]`. That producer must explicitly request the
development-artifact path for its resulting SHA. A bounded reconciliation job
must also detect retained-main-history gaps caused by skipped/canceled events
and request missing builds, so the every-main-commit promise does not rely solely
on a push workflow being started. Record each promotion by SHA and make retries
idempotent; surface a summary of gaps rather than silently selecting only main tip.
Use a checkpointed first-parent traversal starting at rollout, advance the
checkpoint only after each request is durably recorded, and resume unfinished
pages on later runs. Do not enumerate every historical main commit or rescan
expired package records as if they had never been produced. This backfill work
is outside required merge-group checks and does not publish verdicts for other SHAs.

## Nightly, release and infrastructure workflows

Use the same build producer and artifact validation for nightly and release, but
request their actual release profiles, effective version-stamped source and Linux
compatibility baseline. Existing debug CI binaries cannot substitute for these.
Keep macOS signing/notarization, complete-package verification and existing
nightly/release publication behavior. Benchmark may share an identical release
profile only when source/version, build environment and features really match.

`ci-image.yml` builds the environment, not Elephc, and retains its image build
responsibility. Metadata-only workflows, queue cleanup and audits that do not
consume Elephc need no compiler artifacts. No new release publication is introduced
by the development-artifact work.

## Delivery and acceptance

Implement as three reviewable stages of the same change program:

1. Producers, artifact validation and host consumers, including the ten iOS host
   compiler rebuilds. Keep required gates working throughout the migration.
2. Consolidated event ownership plus PDO/SDK and special-profile test archives;
   unify scheduled/manual, nightly and release producer usage.
3. Exact-SHA main reuse, retained development downloads, consumer audit gate and
   before/after CI measurements. Final acceptance requires all three stages.

Acceptance requires the same planned tests, selectors, features and target coverage;
failed/canceled required builds must prevent green final checks; missing, corrupt,
expired and mismatched artifacts must take the documented failure/rebuild path.
Add specific regression cases for exact required check names, both PDO verdicts,
PR versus queue SHA mismatch, two concurrent queue entries, same-PR rebuilt groups,
HEADGREEN multi-entry merges, canceled producers, partial rerun attempts, child
concurrency collisions, expired artifacts after a green gate and a main push while
optional queue jobs are still running. Verify graph dependencies for cycles before
enabling the consolidated path. Required aggregate checks must complete as failures
when their build fails, rather than remaining skipped/pending or becoming success.
Validate PR, extended PR, merge-group, main reuse, direct main build, scheduled,
manual, nightly and release entry paths. Test the release path's preparation and
selection without creating a release solely to test the refactor.

Run focused planner, manifest, download and graph tests locally. Use real CI for
the host/SDK/iOS matrix. Measure build invocation counts, summed runner minutes,
critical-path duration, transferred/stored artifact bytes and cold/warm behavior
against comparable baselines. No percentage saving is promised before measurement.

The unmodified baseline planner and both cleanup shell suites passed during this
design review. They mock GitHub calls and do not validate the proposed new graph.
End-to-end compatibility remains an implementation acceptance gate.

Budget estimate: 3-5 working days including implementation, focused validation,
CI integration fixes and documentation. This is broader than the earlier 1-2 hour
estimate for uploading already-built development packages alone.
