---
id: gotcha-one-absent-function-blanks-every-web-route-and-t-4f702c34
type: gotcha
title: "One absent function blanks every --web route, and the fatal names the wrong one"
description: "An absent function in an eval fragment is an uncatchable fatal that kills the worker before the flush, so all routes return zero bytes; the framework error handler then enhances the error and the enhancer's own missing function is what the message names. ELEPHC_EVAL_TRACE=all plus a grep for UnsupportedConstruct names the real first link in seconds"
created: 2026-09-21
verified_by: "Traced on the Symfony --web worker: exactly one distinct UnsupportedConstruct name in the whole trace, and the previous binary reproduced 7/7 with both functions still absent"
sources:
  - path: src/linker/bridges.rs
    blob: 115539562593f29546067f6c27226f89fd2c1b8b
    lines: 700-720
    snip: f0a9b2ed2058
    anchor: "fn magician_curl_missing_error_if_absent(&self, filename: &str) -> Result<PathBuf, LinkError> {"
---

# One absent function blanks every --web route, and the fatal names the wrong one

## Fact

An absent function reached from an eval fragment is an UNCATCHABLE
`Fatal error: eval() fragment uses an unsupported construct`. Under `--web` that kills the worker
BEFORE the response is flushed, so EVERY route returns zero bytes — not just the route that
triggered it. A whole-application blackout can therefore come from one missing builtin on a path
that only runs at request END.

WORSE, THE ERROR MESSAGE NAMES THE WRONG FUNCTION. A framework's error handler typically tries to
ENHANCE the resulting error, and the enhancer itself calls functions. So the crash you read names
whatever the enhancer needed, not what actually broke. Measured example: the reported fatal named
`substr_compare`, the real blocker was `error_get_last`, and `substr_compare` was only reached
because `UndefinedFunctionErrorEnhancer` ran after the first failure. Implementing the named
function is real work that does not fix the outage.

THE DIAGNOSTIC THAT CUTS THROUGH IT, in seconds rather than a rebuild cycle:

    ELEPHC_EVAL_TRACE=all ./the-web-binary ...            # serve one request
    grep -o 'name="[^"]*" status=UnsupportedConstruct' trace.log | sort -u

That prints every distinct construct the interpreter refused, so the FIRST link of the cascade is
visible next to the last. Run it before and after any fix of this class; an empty result is the
only proof the chain is clear.

THE SECOND-ORDER LESSON, which is why this cost a day. Making a dormant feature WORK can expose a
chain of absent functions that nothing reached while it was dormant. `register_shutdown_function`
was inert; the moment it ran, Symfony's `ErrorHandler::handleFatalError` started executing at
request end and called `error_get_last`. The binary built the previous evening passed 7/7 while
BOTH functions were missing, precisely because neither path ran. So:

- Do not read a passing gate as evidence that the functions on a newly-enabled path exist.
- When enabling a dormant hook, trace one request with `ELEPHC_EVAL_TRACE=all` FIRST and read the
  refusals, rather than waiting for the suite to fail.
- A 0/7 gate right after landing a feature is a cascade to trace, not necessarily a regression in
  the thing just landed. The last two agents each correctly refused to accept blame for it.

ALSO KNOWN ABSENT AND ON THIS CHAIN: `get_defined_functions()`, reached once the enhancer actually
enhances an undefined-function error (it appears in `src/prelude_prune/usage.rs` only as a NAME in
the symbol-table-probe list, never as a contract).

AND A BUILD TRAP THAT IMITATES THIS EXACT SYMPTOM: `src/linker/bridges.rs:709` probes
`current_exe()`'s directory, then `$CARGO_TARGET_DIR/{debug,release}`, then `target/debug` BEFORE
`target/release`. An `elephc` built into a custom `CARGO_TARGET_DIR` can link a STALE
`libelephc_magician.a` and report `call to undefined function <your new builtin>()` while the
source is correct. Build `-p elephc-magician` into your own target dir too. It cost one agent two
full rebuild/diagnose cycles.

## Why

Enabling a dormant hook exposes a chain of absent functions nothing reached while it was dormant: the previous evening's binary passed 7/7 while BOTH were missing, because neither path ran. A 0/7 right after landing a feature is a cascade to trace, not proof the feature regressed.
