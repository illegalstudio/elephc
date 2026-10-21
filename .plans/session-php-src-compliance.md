# PHP-src session compliance campaign

- [x] Publish upload-progress lifetime regressions independently: upstream PR #1965.
- [x] Isolate this campaign at origin/main `f5b378f433feb0d8b0be7f352038f202425c77a9`.
- [x] Freeze the primary normative source and inventory the complete session corpus.
- [x] Build and fingerprint the matching PHP oracle; current final Elephc evidence follows below.
- [ ] Record API, INI, lifecycle, persistence, serialization, handler and HTTP gaps.
- [ ] Reproduce and fix root-cause families with native HTTP regressions.
- [x] Replay/inventory all 263 original PHPTs, retaining adapters and every failure.
- [ ] Remove original-source syntax/adapter blockers and validate native PHPT behavior.
- [ ] Validate supported PHP profiles and target matrix; reconcile documentation.
- [x] Rebase and publish the bounded correction slice with its remaining gates stated.

## Normative baseline

Primary baseline: php-src tag `php-8.5.11`, commit
`678778973bb4d4185c06ef1320de50f1a975d3c8` (annotated tag object
`597eb0b4e32cc8802e112fe13cc92fb5636a1781`). The official release archive
`php-8.5.11.tar.xz` has SHA-256
`d9be75c08e8c316f4c8f4194d8fbe1750a15f6a6d9d4e3fe72082abeeb800360`,
verified before extraction. Runtime source, stubs and tests are read from it.
The initially installed PHP 8.5.10 is not the normative execution oracle.

The corpus contains **263** PHPTs, with no filename exclusions. Inventory and
per-test hashes live in `.artifacts/session-compliance/inventory.json`.
Sections include 178 SKIPIF, 166 INI, 13 COOKIE, 12 GET, 14 POST_RAW,
2 EXPECTHEADERS, 1 CGI, 14 CLEAN and 1 XFAIL. There are 116 EXPECT and
147 EXPECTF expectations. These are inventory counts, not passing results.

## Scope and acceptance

Audit all session functions, constants, interfaces and SessionHandler methods in
`ext/session/session.stub.php`; registered INI directives and validation in
`session.c`; files storage and locking in `mod_files.c`; all three serializers;
errors, exceptions, callback order, cross-entry identity/references, malformed
input, lifecycle/shutdown, cookies/cache headers, strict mode, GC, lazy writes,
SID sources, trans-SID and multipart upload tracking.

The main public implementation is `src/web_prelude/build.rs`, with the shared
contract in `crates/elephc-builtin-contract`, file/state/wire helpers in
`crates/elephc-web/src/session`, and HTTP regressions in
`tests/web_session_tests.rs`. New PHP-visible behavior must keep the EIR path
and all existing PHP profile semantics authoritative.

No declaration inventory or focused green test can establish 100% compliance.
Complete acceptance requires a current complete replay with every test accounted
for, resolved behavior/adapter gaps, appropriate native ownership evidence,
PHP 8.2/8.3/8.4 profile checks against frozen secondary sources, supported-target
coverage, and accurate user documentation. Existing standalone-web/CLI, dynamic
module and diagnostic-channel differences remain explicit gaps rather than
being silently removed from the denominator.

## Initial source findings requiring reproduction

- `session_decode()` currently returns true after every active-session decode;
  the decoder has no failure result. Invalid serialized session data cannot
  propagate PHP's false return and session cancellation correctly.
- The files branch of `session_reset()` reads a saved in-memory snapshot rather
  than rereading the held session file. Custom-handler read failures are cast to
  empty strings, also hiding failure.
- `php`/`php_binary` encode/decode serialize each entry independently; shared
  references and object identity across entries need explicit comparison.
- Numeric-key behavior and inactive-session warning behavior need differential
  probes, not just byte-format unit checks.
- iOS device/Simulator compilation already fails in broker platform helpers at
  the initial upstream SHA. It remains a target acceptance blocker, independent
  of the completed upload-progress lifetime audit.

## Evidence protocol

Record binary/source/archive hashes, commands, per-case original FILE/INI,
stdout/body/stderr/headers, compiler diagnostics, exit/HTTP status, timeouts,
SKIPIF/CLEAN results and root-cause classifications. Never execute oracle PHP
as a native fallback. Keep full-corpus and filtered results distinct. Preserve
source files and fixtures unchanged for PHPT replay; adaptations must be named
and cannot be represented as unchanged upstream passes.

## Earlier implementation checkpoint (before the resumed validation)

At this checkpoint the compliance work was still local on
`feat/session-php-src-compliance`; test-only PR #1965 had been published independently. The first 7 HTTP regressions passed, 65
session unit tests and 4 contract-support tests passed, and 3 existing native
Core error-handler regressions passed. A later 10-case expansion exposed scalar
session-root storage and writable R-reference gaps; these are being corrected,
not waived. No 100% result is claimed.

The current implementation adds a named superglobal representation (`_SESSION`
as Mixed, the other request globals as hashes) consistently across checker,
EIR, global stores, eval synchronization, frame cleanup and request reset.
The shared unserializer now promotes uppercase-R registry cells in place using
owned payload transfer. Persistent-reference hash writes and constructor bucket
replacement are being aligned on both architectures. These latest changes still
require the final native/heap/target checks.

The raw original-source replay was fully inventoried (263 cases), but remains
blocked on syntax, SAPI/request adapters and oracle-adapter fidelity. The upstream
PHP runner itself passed 242 cases, skipped 20 and recorded one expected failure.
The custom runner has since received INI quoting/default and pristine-fixture
fixes; retain earlier exploratory results rather than confusing them with a
final replay.

## Resumed validation on 2026-10-21

Rebased the retained dirty worktree onto origin/main
`d283bc1349` (0.27.4), preserving all tracked and new source files.
No upstream session/runtime implementation overlapped this rebase.

The writable completed-array R-reference regression now passes after preserving
callee-saved registers and transferring an incoming ordinary Mixed owner directly
into the reference. A write-context fetch still borrows that exact box until
hash_set returns: cloning and releasing it inside the assignment helper is invalid.
An incoming persistent reference is copied by value before publication. Constructor
writes continue to detach a bucket rather than mutate the previous reference set.

Before the rebase, 10/10 new HTTP cases passed, including php_serialize integer,
false, and null-root normalization. The repeated ordinary/native tagged GC test
passed; the heap probe balanced 1,000 allocations and 1,000 frees. Native Linux
ARM64 gave the same result. An x86_64 link probe exposed a misspelled return label;
the source fix is included and final regenerated native evidence is still pending.
An assembly-only diagnostic correction also passed the x86_64 heap probe; this is
not substituted for a final source-built result.

Open reference gates include scalar/string references, references to unfinished
arrays, lowercase-object reference numbering, reference-preserving merge into a
nonempty session, and cross-entry encoding identity. The shared-buffer conversion
and one completed-array mutation regression do not close these broader gates.
The PHP 8.5.11 decoder normalizes a serialized null root to an empty session array;
that behavior is now covered together with scalar roots.

The runner now permits a nonzero reference exit when its complete expected output
matches: PHP's fatal-error PHPTs intentionally exit 255. A real-PHP runner regression
covers this. Upstream XFAIL remains an explicit `oracle_xfail` outcome.

## Final bounded-slice evidence

After the rebase, the complete `web_session_tests` binary passed **64/64** tests,
including all 10 new regressions and existing 8.2/8.3/8.4/8.5 profile, custom-handler,
request-reset, persistence and isolation cases. Session crate units passed **65/65**;
the final contract-support selection passed **3/3**. The expanded GC regression
compares nested writes, complete-array and scalar replacement, plus ordinary COW,
against the matching PHP oracle. Native macOS and both Linux architectures balance
1,160 allocations/frees. The iOS device/Simulator gates assemble both program and
runtime; they are not execution or full web-bridge acceptance.

The stable final raw replay is retained in
`scripts/session/baseline-8.5.11.json`: **242 compile errors, 20 adapter errors,
1 oracle XFAIL, 0 oracle errors, 0 native PHPT executions**. All 263 original FILEs
and hashes are retained; nothing was filtered or source-normalized. Compiler
blockers group as 229 Question tokens (closing tags), 6 Ampersand tokens,
5 empty Semicolon statements, 1 Greater token, and 1 unsupported reference array
literal. Source adapters include request/header sections and the CGI fixture/SAPI.
The full upstream PHP runner independently gave 242 pass, 20 skip, 1 XFAIL.
Those reference-runner counts and our adapter counts are distinct measurements.

The final replay uses a frozen copy of the compiler and bridge archives; end
fingerprints are checked. Compiler SHA-256:
`ef259375075e981581f09c101e72aef1db4c7b098b1bcd919afd7c04b18767c7`.
The corpus report contains source/binary/bridge/runner hashes and each result.
Diagnostics can still show synthetic prelude line numbers. Secondary-profile
smoke cases passed, but independent frozen 8.2/8.3/8.4 oracles remain open.

Declaration comparison also identifies named-argument contract discrepancies in
`session_regenerate_id`, `session_set_save_handler` and `session_set_cookie_params`;
several catalog results use broad Mixed storage instead of the precise upstream
return union. Nominal coverage of 23 functions and 4 classes does not settle these
behavior/reflection contracts. `SID` is a dynamic constant, not a fourth startup
constant in the oracle snapshot. `session.trans_sid_tags/hosts` belong to standard,
not the session extension's INI-filtered inventory.

Next slices should unblock original-source parsing in a dedicated baseline-fix
branch, implement the missing request/CGI adapters faithfully, then repair named
arguments and the remaining serialization/reference families using the retained
failure inventory. Full session compliance is explicitly not achieved by this PR.

Final post-rebase rerun: all **85/85** selected unserialize tests passed from
`codegen_tests-365b4066e241bccc`, including the expanded GC regression. All tested
source fingerprints in the checked-in replay report still match the worktree.

PR #1965 was then merged as `b96e3ac8f8be28164aa91dca3444bcc6a41f8262`.
Rebased this slice on that commit. Its only Rust production-file edit registers
the two cfg(test) upload-lifetime regressions; runtime implementation is unchanged.
The focused session units were rerun and passed **67/67**, including both newly
merged cases. The earlier exact replay remains pinned to its original artifacts.

The bounded correction slice is now published from the user's fork as draft
PR https://github.com/illegalstudio/elephc/pull/1966 on
`feat/session-php-src-compliance`. Full compliance and the remaining acceptance
gates remain open. CI for that PR must be checked at its current head, separately
from the successful and now merged test-only PR #1965.
