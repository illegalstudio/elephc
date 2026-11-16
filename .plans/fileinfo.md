# PHP fileinfo

- [x] Add pinned static libmagic with its embedded database on all five targets.
- [x] Implement the six functions, finfo class and eleven constants in AOT and eval.
- [x] Cover custom databases, resources, wrappers, filters, flags and ownership.
- [x] Add examples, generated documentation and complete host/iOS CI coverage.
- [x] Open PR2061 and mark it ready for review as requested.
- [ ] Obtain clean Grok, Greptile 5/5 and green CI on one final head.

PR: https://github.com/illegalstudio/elephc/pull/2061, OPEN and ready.
The user authorized continuing fixes and pushes while ready, and waived the
five-round external review limit. Do not merge the PR. Do not run local builds,
compiled tests or formatters. All executable validation runs in CI.

## Implementation

The optional Rust bridge owns independent libmagic 5.46 handles and embeds the
complete compiled database. Synthetic PHP declarations use the ordinary EIR,
method and extern paths. Default programs need no runtime library or database.

URL queries prepare filters before stream stat, preserve directory identity and
perform magic_stream's separate stat/cast phase. Resource queries perform only
the inner stat and restore their position on normal return. Read and write
filters suppress casts after successful attachment. Query-local resource and
filter state supports reentry. Temporary flags span callbacks and restore in
finally. Close and exception ordering follow the PHP 8.5 reference fixtures.

Cast result records preserve typed native return representations and ownership
until warnings and TypeError construction finish. The record is retired before
value destruction. Error-handler masks control exception-time warnings.

## Review and validation

Nine complete Grok reviews ran through Code Journal in read-only mode. Round 9,
54a23ffd-a9c8-41b2-9c00-ec10e18a2c4d, returned clean on ancestor `791ba53cf5`.
The full round table and evidence-backed declines are in the PR and CJ plan
1a3d36c8-b75c-4a4c-827d-32997e29e737. Later CI and Greptile fixes require a
fresh full verdict. The user explicitly authorized continuing beyond five rounds.

Greptile confirmed 5/5 on `136c176b2e` with all nine threads resolved. Its ARM64
class-ID scratch finding was fixed before that review. Source and executable
evidence remain separate: CI run 40242925341 ran all 46 then-current fileinfo tests,
with 34 passed and 12 failed on Linux x86_64 and 35 passed and 11 failed on Linux ARM64. All 8 native
tests and the production example passed on both Linux targets. Exporter parity
also passed. Those failures are being corrected before a new complete review.

The current candidate makes every filter-state array contract explicit,
aligns wrapper fixtures with the raw string callback ABI, retires raw string
argument temporaries after boxed user returns, and adds focused heap regressions. Returning a static property retains its
source owner independently of the caller, including stream-cast results.
Connection-refused tests use valid FTP file paths. Captured programs must exit
successfully, with stderr included when a failure interrupts their trace.

Generated warning/deprecation operands now query the nearest source call site
through the existing activation chain. Frame-sensitive optimization gates retain
the needed frames. A new regression covers direct, nested and callable
finfo_close, buffer deprecation, stream warnings and default diagnostic output.

There are 47 fileinfo surface tests, 8 native tests, three focused ownership
regressions and a TLS regression. PHP 8.5.12 reference oracles and safe static
documentation/site, assembly-comment and diff checks pass. The latest candidate
still needs compiled CI validation and a full current-head Grok verdict.

Round 10 attempts bcf47f53-67d3-48af-b5af-72b7076ccfa1 and
22e203aa-0aa3-4536-a5c2-645755dbbedf were cancelled before source corrections
and have no verdict. All guest processes stopped before edits. No local compiled
tests were run. Task-owned oracle files and servers were removed.

The latest execution on abfd283de ran47 surface cases on each Linux host,
with38 passing and9 failing. Filter callback traces now match. Remaining failures
primarily identify string owners retained by wrapper/filter registrations and a
filtered byte accumulator. Registry rows now release both persisted strings on
unregister and at normal executable shutdown, after resource close callbacks.
The filter reader retires its fallible boxed input before maintaining a separate
string byte accumulator. Diagnostic line tests use a static expected value to
avoid an unrelated global-reference capture backend limitation. New cases and
these corrections await fresh CI; no local compiled tests ran.
