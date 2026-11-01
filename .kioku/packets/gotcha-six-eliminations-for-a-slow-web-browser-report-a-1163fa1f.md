---
id: gotcha-six-eliminations-for-a-slow-web-browser-report-a-1163fa1f
type: gotcha
title: "Six eliminations for a slow --web browser report, and php -S as the control"
description: "Connection setup, keep-alive, browser headers, gzip, body download and worker count are all measured to have no effect on --web latency, and there is NO fork per request despite a subagent claiming one. Use php -S interleaved as the control because it is stable at 1.3ms across every machine load, and never quote a latency without --keep-symbols attribution"
created: 2026-09-21
verified_by: "Six A/B probes on one binary, a reversed second pass for worker count, pgrep process counting across six requests, and php -S measured at loads 12 through 21"
sources:
  - path: crates/elephc-web/src/server.rs
    blob: 5a3ea3cdade78c3eb47acb997827e93e32eb988b
---

# Six eliminations for a slow --web browser report, and php -S as the control

## Fact

When someone reports a slow `--web` response from a BROWSER, these are eliminated already. Each
was measured, not reasoned; do not spend a cycle on them again.

    connection setup            0.14 ms      negligible
    Connection: keep-alive      no effect    23.44 vs 26.52 ms, inside the spread
    browser request headers     no effect    22.64 ms with the full Chrome header set
    Accept-Encoding: gzip       no effect    26.62 ms
    TTFB vs total               identical    the body download costs nothing
    --workers 1 / 4 / 18        no effect    21.49 / 21.20 / 21.06 ms min, and a reversed
                                             second pass agreed, so it is not drift

THERE IS NO FORK PER REQUEST, despite a subagent report saying there was. Counted with `pgrep`
before, during and after six requests: the process count stays at master + N workers, unchanged.
What that agent actually observed -- a PHP function `static` reading 1 on every request -- comes
from the deliberate `reset_retained_eval_contexts` at the request boundary, not from a new
process. The profile of the worker is therefore VALID: it samples the process doing the work.

USE `php -S` AS THE CONTROL, ALWAYS, INTERLEAVED. Absolute numbers here move by a factor of two
with machine load, so a number alone says nothing. php on the same app, same instant, same
machine measured 1.28-1.32 ms ALL DAY across loads from 12 to 21 -- it is a small, stable process
and it barely notices the weather. So when elephc's number moves and php's does not, the machine
is not the excuse. Measured at load 12:

    /        elephc 21.65 ms   php 1.28 ms    17x
    /plain   elephc 14.34 ms   php 1.26 ms    11x

DO NOT QUOTE THE SUB-10ms FIGURES FOR `public/index`. They belong to a different binary: the
`index_preload_hotpath` variant, whose packet states plainly that even there `/` stays interpreted
because `twig/twig` does not compile, so `Environment`, `Template`, the escaper and the generated
template class all run in the interpreter. Re-measure on the binary in front of you before
quoting any latency; I repeated those numbers in a demo without re-verifying and they were wrong
for the build being demonstrated.

A PROFILE WITHOUT `--keep-symbols` CANNOT ATTRIBUTE THIS. The binary carries 275 symbols and every
compiled PHP frame prints as a raw address: on a 15 791-sample run the malloc/free family was
17.2% and memmove/memset/bzero 9.8%, but **68% was unattributable `???`**. Build the profiling
twin with `--keep-symbols` (982 397 symbols) BEFORE drawing any conclusion. Use
`scratchpad/build-prof.sh`, which writes `public/index_prof` instead of clobbering `public/index`
-- other agents verify against that file and overwriting it mid-run invalidates their gate.

## Why

Absolute numbers here swing by a factor of two with load, so a number alone proves nothing; and a profile without --keep-symbols left 68% of a 15791-sample run as unattributable raw addresses.
