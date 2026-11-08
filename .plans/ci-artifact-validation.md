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

Reproduce the inventory with:

```bash
python3 -B scripts/ci/measure_run.py 42439918696 42439918652
```

## Focused checks

- 53 Python infrastructure tests cover archive integrity, identities, producer
  attempts, incompatible environments, dependency graphs, the actual gate scripts,
  partial matrix reruns and development reconciliation crash recovery.
- The CI tier planner and both queue cleanup shell suites pass.
- Nextest archive validation and the isolated prebuilt documentation exporter
  checks pass. The full docs extraction suite is not needed for this change.
- Focused Rust regressions for the PDO feature marker, OpenSSL bridge override and
  complete bridge build inventory pass through the machine-wide build queue.
- actionlint 1.7.12 and whitespace checks pass.

## Remote integration

The initial integration runs found and fixed container Git ownership, multi-path
container upload mapping, specialized nextest includes, default sh versus Bash,
and missing nextest extraction directories. The iOS producers, specialized tool
producers and Linux x86_64 host producer have completed successfully. The full
consumer matrix remains under validation on the latest PR head. Curl contract
consumers have also passed using only their prebuilt test payload.

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
