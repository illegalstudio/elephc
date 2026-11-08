---
title: "Shared CI artifact validation"
description: "Measured baseline and validation evidence for PR 2282."
---

# Shared CI artifact validation

Implementation: https://github.com/illegalstudio/elephc/pull/2282
Baseline source: `b62bff4ca643d7b7b6019f3d563253a8b918bb1f`.

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
