---
id: runbook-web-isolation-is-a-build-flag-and-request-is-wha-c728abc5
type: runbook
title: "--web-isolation is a BUILD flag, and 'request' is what makes the Symfony Twig route serve every request"
description: "Passing it to the binary does nothing; it is baked into the process-entry stub at compile time"
created: 2026-09-18
verified_by: "verify-bin.sh index_iso: all eight route checks IDENTICAL to php -S including / twice, 30/30 timing samples returned 200; the same entry built with the default worker isolation serves / twice and then segfaults"
sources:
  - path: src/codegen/mod.rs
    blob: 6ac319368505a5718b4e0e9f6aa6a60316d17f28
  - path: examples/symfony-app/public/index_iso.php
    blob: e8b652d89f5a0ff3c110d05122a91cc8517e574b
---

# --web-isolation is a BUILD flag, and 'request' is what makes the Symfony Twig route serve every request

## Fact

    ./target/release/elephc --web --web-isolation request examples/symfony-app/public/index_iso.php

`WebIsolation` picks the bridge symbol embedded in the generated process-entry stub
(`elephc_web_run` / `elephc_web_run_pool` / `elephc_web_run_request`), so the mode is fixed when the
BINARY is built. `./index --listen … --web-isolation request` is silently ignored -- I lost a test
cycle to that.

MEASURED on this machine (examples/symfony-app, 1 worker, warm):

| entry                  | isolation | `/` (Twig)              | `/plain` |
|------------------------|-----------|-------------------------|----------|
| index_preload_hotpath  | worker    | 33 ms, then SIGSEGV on request 3 | 10.9 ms |
| index_iso              | request   | **72.6 ms, 30/30 ok**   | 44.7 ms |
| `php -S`               | -         | 1.5 ms                  | -       |

So a fresh handler process costs ~34 ms here, and the interpreted Twig render ~22-28 ms on top of a
compiled route's ~11 ms. Under 10 ms for `/` needs BOTH: the worker mode (no fork) and Twig in the
closed world -- `index_preload_twig` compiles but does not serve yet.

The worker mode's remaining blocker is a SIGSEGV on the THIRD `/` request
(`EXC_BAD_ACCESS at 0xffffffff00000010`, a tagged word used as a pointer). macOS writes a full report
to `~/Library/Logs/DiagnosticReports/<binary>-*.ips`; decode it with `json.loads` after the first
line. The frames carry image offsets only -- the emitted local labels are not in the symbol table --
so `atos` gives nothing and `sample` on a LIVE worker is the way to name them.

## Why

It is the difference between a Twig route that works once per worker and one that works every time
