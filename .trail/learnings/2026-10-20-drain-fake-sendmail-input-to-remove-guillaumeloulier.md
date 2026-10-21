---
title: Drain fake sendmail input to remove the CI pipe race
author: Guillaumeloulier
date: 2026-10-20
branch: fix/hash-destruction-insertion-order
touches:
  - tests/codegen/strings/mbstring_startup.rs
---

## Decided
A successful fake mail transport must consume stdin before exiting. Use /bin/cat for the mb_send_mail warning-handler integration fixture; stdout is already discarded by the transport. Preserve both complete warning messages, their order, the final sent result and clean-heap assertions. Include the actual exit status in the shared fixture failure message.

## Tried and ruled out
Linux x86_64 shard3 failed twice after printing the two expected unsupported-encoding warnings, before the sent marker or heap summary. The fake /usr/bin/true closes stdin immediately. An exact current-compiler glibc program reproduced the same output and SIGPIPE(-13) when a preload shim delayed only writes to non-stdio pipe descriptors, representing a legal scheduler interleaving. Five hundred unperturbed repetitions passed, so a few green reruns do not establish race freedom.

With /bin/cat, all60 Linux glibc runs passed, including10 delayed-writer runs, with exact warnings, sent output and clean heaps. AOT/eval on macOS passed in both EIR modes using frozen compiler/bridge snapshots. Do not remove warnings, permit failed delivery or relax heap checks to hide this race.

## Watch out
A test transport that exits without reading does not model successful message acceptance. Blocking/ignoring SIGPIPE in production is a separate process-I/O policy question; this CI repair changes only the fake transport and diagnostics. It is unrelated to the hash destructor-order runtime source. Preserve the forced-interleaving witness and raw job logs, update the existing fork PR with an exact lease, and verify new-head CI rather than Greptile or a superseded run.
