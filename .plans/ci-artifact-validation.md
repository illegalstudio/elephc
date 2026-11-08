---
title: "Shared CI artifact validation"
description: "Measured baseline and validation evidence for PR 2282."
---

# Shared CI artifact validation

Implementation: https://github.com/illegalstudio/elephc/pull/2282
Original baseline: `b62bff4ca643d7b7b6019f3d563253a8b918bb1f`.
Integrated main baseline: `145426b7fe5cc0fd60f5d56a9d7a1c9c745280b5`.

## Completed build-sharing validation at e17a086

[Run 42499694570](https://github.com/illegalstudio/elephc/actions/runs/42499694570)
passed the full extended matrix on `e17a086444f09ecbc80b9d3c24b82dd2cfd0f0dc`:
282 successful jobs, 15 expected skips, all three required top-level gates, all six
PHP SDK consumers, ten iOS compile/link jobs and 253 consumer compilation audits.
CodeQL passed too. Comparing 193 JUnit reports against the integrated baseline
found 93,927 existing test identities retained and nine added identities.

Elapsed time was 80.58 minutes, with 3643.53 summed runner minutes. The 14 selected
producer jobs recorded 18 root-package compile events versus the baseline's 85.
This is observational evidence, not a controlled cold/warm benchmark.

The run uploaded 225 artifacts totaling 10,169,916,929 bytes, including 8.47 GB of
workspace/codegen/PDO test archives. This is the temporary peak before lifecycle
cleanup, not the intended retained download size. The three compiler and bridge
payloads total about 1.1 GB. An actual Linux package from this run passed the
standalone crypto compilation/link probe outside the checkout with zero Cargo
builds; its tarball was 372,861,967 bytes.

## Latest-only retention revision

The user selected three complete packages and latest-main-only retention after
the full run above. The lifecycle revision removes completed-run intermediates,
preserves active consumers and minimal queue handoffs, orders packages by main
history, and renews the retained bytes before GitHub expiry. Its focused tests
cover incomplete replacement, old late completions, queue consumers, rerun races,
API failure, durable selection and package integrity. The original full-run result
does not by itself validate this later revision; rollout still requires observing
the privileged default-branch completion and scheduled workflows after merge.

Local revision checks pass: 84 Python infrastructure tests, the actual CI tier
planner, both queue cleanup suites, nextest archive validation, Action pin checks,
workflow actionlint and ShellCheck on the three edited/new retention workflows.
The minimal queue handoff regression fails on the previous resolver and passes
after removing disposable test archives from the packaging prerequisites.
A read-only dry run against `42499694570` proposes deleting all 225 PR artifacts
(10,169,916,929 bytes) and retaining none. No GitHub artifact was deleted during
development or this validation. Main fixtures retain exactly three packages.

## Baseline observed on 2026-11-08

| Run | Event | Conclusion | Elapsed minutes | Sum of runner minutes | Artifact bytes |
| --- | --- | --- | ---: | ---: | ---: |
| [42439918696](https://github.com/illegalstudio/elephc/actions/runs/42439918696) | merge_group, CI including SDK | success | 70.47 | 3396.05 | 4,444,604,849 |
| [42439918652](https://github.com/illegalstudio/elephc/actions/runs/42439918652) | merge_group, PDO | success | 20.02 | 107.92 | 0 |

Both workflows ran concurrently. Their combined runner time is 3503.97 minutes;
elapsed time is not the sum. This is wall-clock job time without platform billing
multipliers. Stored artifact bytes are not a measurement of network transfer.
Host build/archive jobs took 6.72 minutes (Linux x86_64), 9.10 minutes (Linux ARM64)
and 26.93 minutes (macOS ARM64). These observations do not establish cold-cache cost.

The logs of 29 compilation-bearing CI jobs contain 67 Cargo `Compiling elephc v`
events; seven PDO jobs contain another 18. This includes six events in each of the
six PHP SDK jobs and one in each of the ten iOS jobs. The total of 85 counts root
package compilation events, not individual rustc invocations. Test-shard logs were
not included in that inventory.

Reproduce the inventory with:

```bash
python3 -B scripts/ci/measure_run.py 42439918696 42439918652
```

## Baseline after integrating SimpleXML main

The final implementation includes main `145426b7fe5cc0fd60f5d56a9d7a1c9c745280b5`.
Its successful merge-group runs provide the source-comparable timing baseline:

| Run | Coverage | Elapsed minutes | Sum of runner minutes | Artifact bytes |
| --- | --- | ---: | ---: | ---: |
| [42485938535](https://github.com/illegalstudio/elephc/actions/runs/42485938535) | CI including SDK | 84.12 | 3592.08 | 4,459,671,663 |
| [42485938603](https://github.com/illegalstudio/elephc/actions/runs/42485938603) | PDO | 19.63 | 109.35 | 0 |

Combined runner time is 3701.43 minutes. Host build/archive jobs took 13.58 minutes
(Linux x86_64), 12.40 minutes (Linux ARM64) and 35.38 minutes (macOS ARM64).
The same 36 selected jobs in these newer runs also contain 85 root-package
compilation events: 67 across 29 CI jobs and 18 across seven PDO jobs. Neither
baseline controls cache state or hosted-runner load.

## Focused checks

- 59 Python infrastructure tests cover archive integrity, identities, producer
  attempts, incompatible environments, dependency graphs, the actual gate scripts,
  partial matrix reruns and development reconciliation crash recovery.
  A real Git history fixture also rejects pre-rollout commits, merged PR branch
  heads and unmerged branches at the development dispatch trust boundary.
- The CI tier planner and both queue cleanup shell suites pass.
- Nextest archive validation and the isolated prebuilt documentation exporter
  checks pass. The full docs extraction suite is not needed for this change.
- Focused Rust regressions for the PDO feature marker, OpenSSL bridge override and
  complete bridge build inventory pass through the machine-wide build queue.
- The nightly curl packaging regression and a real prebuilt libtest runner smoke
  pass. All 17 PDO codegen argument lists match the old workflow exactly.
- actionlint 1.7.12 and whitespace checks pass.

## Remote integration snapshot, run 42488921072

The initial integration runs found and fixed container Git ownership, multi-path
container upload mapping, specialized nextest includes, default sh versus Bash,
and missing nextest extraction directories. Every host, iOS and specialized
producer completed successfully. This snapshot precedes final-head validation;
the PR records its latest result. Curl contracts,
documentation, tokenizer, differential checks, release frame assertions, all four
Linux PHP SDK jobs and all optional PDO driver consumers have passed using their
prebuilt payloads. Sampled completed consumer logs contain no root-package compile
events, and their consumer build audits passed.

All three host producers record two root-package compile events each, nine PDO
producers record one each, and the two specialized producers record three in total:
18 events across the selected build jobs, versus 85 in the baseline inventory.
This is not a measurement of total CI elapsed time or billing savings. The core
PDO gate, optional PDO gate and Build gate passed, with their expected check names.
XML, Fileinfo, GMP and curl links passed on both iOS targets.

Run `42488921072` exposed a migrated timeout: the 98-case GMP corpus had completed
in 213.53 seconds under the old Cargo runner, but nextest terminated it at its
ordinary 180-second single-test limit. An exact corpus override now allows ten
minutes without removing any cases. CodeQL also identified the historical checkout
as input-controlled; packaging now independently validates the retained main
first-parent SHA before checkout, in addition to validation in its caller. The
nightly curl assertion now follows the shared producer, and tokenizer iOS jobs
restore their target bridges before linking; a graph test covers all iOS consumers.

The first migrated MySQL run passed all 137 tests in 1144.838 seconds, versus
699.90 seconds under the old libtest runner. This is not a controlled benchmark,
but the migration had changed one shared process into a process per test. PDO
codegen selections now execute the archived binary in one process to preserve
the original in-memory runtime caches, thread counts and fixture watchdogs.

A downloaded Linux x86_64 payload from producer run `42486034417` passed the actual
standalone package probe locally. Its synthetic PR merge source was
`18de6bd1ce4196a8fe91c0f3a41a1dcdcf7451ac`, verified against the GitHub commit tree.
The generated development tarball was 372,038,625 bytes. The probe compiled, linked
and executed a SHA-256 crypto program outside the checkout; its temporary files
were removed. This proves that payload's packaging, not the final PR matrix.

No end-to-end success or percentage saving is claimed from cancelled or partial
runs. Final measurements must include the consolidated PDO and SDK coverage, the
consumer compilation audit, artifact storage and the selected tiers. Development
packages must also pass their standalone install probe. Release validation does
not create a new release merely to test the workflow refactor.

## Integration follow-up on main 145426b7

Run `42493865194` passed all producers, both PDO gates, all six PHP SDK jobs, the
GMP corpus and both tokenizer iOS links. Its 14 producers recorded 18 root-package
compile events. All six SDK jobs, MySQL and both tokenizer iOS consumers recorded
zero. MySQL passed the same 137 cases in 868.90 seconds, versus 905.28 seconds on
the matching main baseline; GMP passed in 349.278 seconds, versus 380.78 seconds.
These individual observations do not control runner load or cache state.

The Linux compiler and bridges from that run passed a standalone installation
probe after source-tree and manifest verification. The synthetic source SHA was
`6374af557a00d322666b3ed925a2cda030a330df`, matching the tree of PR head `d350e97dc3`.
The tarball contained 372,861,956 bytes; the crypto fixture compiled, linked and
ran outside the checkout with zero consumer Rust builds. Temporary files were
removed. macOS/ARM packaging, main promotion and scheduled reconciliation still
need their actual rollout execution; no release was created for verification.

A final graph review found that the Linux x86_64 producer omitted the coordinator's
resolved image input and therefore resolved the image a second time. The caller
now passes the same immutable digest as every other coordinated host. A regression
check fails on that omission. This correction requires a fresh exact-head CI run.

The planner regression suite now reads the actual macOS path policy from `ci.yml`
and exercises every reusable workflow called by the coordinator. It exposed that
a PR changing only an extracted workflow would lose the automatic macOS/iOS tier
that editing the former monolithic `ci.yml` enabled. The policy now covers those
`ci-*.yml` files too; the real shell planner fails this check before the fix and
passes afterwards. The 59 Python checks and actionlint also remain green.
