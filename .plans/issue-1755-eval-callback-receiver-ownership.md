# Issue #1755 — eval array-callback receiver ownership

- [x] Reproduce the original leak and implement scoped callable ownership.
- [x] Migrate normalization consumers and preserve transferred receiver ownership.
- [x] Validate ordinary/exception/closure cases and focused adjacent unit tests.
- [x] Replay the original fixtures on macOS and both Linux architectures on/off.
- [x] Run builtin documentation generation and audits.
- [x] Validate curl self-clearing array callbacks and partial frame rollback.
- [ ] Inspect exact-head CI after the review follow-up.
- [ ] Clear the pre-existing iOS pcntl dependency blockers and validate iOS targets.

Base: `32a77fc532184b39ff32cce597f95f1a09f92d51`.
Branch: `fix/1755-eval-callback-receiver-ownership`, isolated and clean at entry.
The refreshed issue is open and no dedicated open fix PR references it.
Main checkout changes and unrelated worktrees are preserved.

## Baseline and design

The environment-selected runtime-eval reproducer leaks four blocks / 688 bytes
after one positional array-callback call and thirteen blocks / 1120 bytes after
ten calls. The initial baseline compiler and bridge use the clean #1828 base
worktree; a compiler was then rebuilt in the #1755 worktree before implementation.

The resolver already returns the owned array-read receiver, but its ordinary
normalization API discards that owner. An explicit `ResolvedEvalCallable` now
carries it into a complete invocation region. The existing argument-ownership
finisher retains borrowed results before releasing the receiver and preserves
an earlier failure if cleanup also fails. Borrowed receivers contribute no owner.

Every normalization consumer is migrated: core and named call_user_func routes,
variable calls, array/regex builtins, iterator callbacks, handler probes and
registrations, curl validation/transfer frames, and signal validation.
Closure::fromCallable explicitly transfers to its existing target cleanup path,
which already retires the original receiver after retaining its bound target.
Curl frames retain their normalized receivers for the entire transfer and clean
both partial setup failures and completed transfers.

## Evidence and boundaries

The fixed original reproducer is heap-clean after one and ten calls. Thirteen
PHP-oracle fixtures cover positional/named/array/variable calls, private/missing
methods, missing arguments, throwing bodies, references, returned receivers,
argument evaluation deleting or throwing away the callback, and closure transfer.
Destructor order and complete stdout are compared, not just successful return.

A separate unknown named method argument already ends in an uncatchable generic
runtime failure on the exact base. Its before/after evidence is retained in
`baseline-unsupported-named-error.json` and `out-of-scope-named-case.json`; it is
not counted as a passing PHP-parity case. Missing-argument binding failures are
covered by the acceptance fixtures.

Artifacts: `/private/tmp/elephc-1755`. Target, regression, unit and generated docs
checks are recorded as they finish. No full local suite is authorized by default.


## Current checks

All thirteen PHP-oracle scenarios pass as three heap-debug integration tests
on/off (runtime source supplied through argv). Adjacent unit tests pass: 49
dynamic-call, 40 operand-ownership and 42 array tests. Curl-feature checking passes.
The bridge staticlib builds for Linux ARM64 and x86_64.

The iOS device bridge build is blocked by sixteen pre-existing errors in
`elephc-pcntl` (errno/signal/priority APIs and platform resource types). The exact
clean base #1828 worktree fails `cargo check -p elephc-pcntl --target
aarch64-apple-ios` with the same sixteen errors. This is not counted as iOS
support or a successful target gate. Logs: `cross-aarch64-apple-ios.log` and
`base-ios-pcntl.log`. No pcntl portability source is changed by this fix.


## Target and docs follow-up

Linux ARM64 passes all thirteen oracle fixtures on/off as real executables with
clean heaps (26 runs). Linux GNU and musl bridge staticlibs build for both
architectures; musl reports the existing two libc::time_t deprecation warnings.
The Docker images use musl, so executable replays link the matching musl archives,
not the GNU archives. The standalone harness uses the compiler's emitted program
assembly and generated eval-enabled core runtime plus magician/bcmath/mbstring/tz.

Builtin export, render, module sections, PHP comparison, catalog/site audits and
EIR architecture audit pass. Generated registries and pages have no diff. The
comparison was additionally run under Docker Python 3.12, after the host Python
3.9 lacked stdlib tomllib (tomli was available for the other host audit run).

The iOS Simulator pcntl dependency check on the clean base fails too; this target
is not reported as validated. Logs remain in the evidence directory.


Linux x86_64 also passes all thirteen fixtures on/off (26 runs), executed under
QEMU on the ARM64 host. Combined Linux executable evidence: 52 PHP-exact runs,
clean heaps. macOS integration coverage is the same thirteen cases on/off.
The iOS device/Simulator dependency failures remain open target gates.


Final host `cargo build -p elephc -p elephc-magician` passes warning-free. PHP
example syntax and diff hygiene pass. The final standalone host compiler/bridge
pair is replayed separately to avoid conflating test-profile and dev artifacts.


Final dev-artifact replay: all thirteen fixtures plus the example pass on/off
(28 executions), full stdout and clean heaps. Compiler SHA-256:
`21b851188cbfc312b5bb2d2acfaeb56bef62c1bf25d9bc1aa8a70583ceb7cd38`.
Host Magician staticlib SHA-256:
`2bb311b3f7af6dfe26b4209666d21061be2a7f8026e839350e197590ce337c89`.
The new regressions and example are validated against both the integration test
profile and the final standalone compiler/bridge pair. Publication remains a
draft while exact-head CI and the explicitly blocked iOS gates are outstanding.

## Review follow-up — curl invocation ownership

Reviewed head: `a1f2f2722ec6939f8e6bbf62fa2697d6ac94f038`.
Open findings: `PRRT_kwDORs6r_s6sN7K2` (plan checklist) and
`PRRT_kwDORs6r_s6sN7K8` (curl receiver ownership tests).

The plan now starts with a checklist, retaining iOS dependency validation as an
open gate. New runtime-source eval fixtures unregister their array-method callback
inside a transfer, read the receiver afterwards, and compare destructor order and
complete stdout against PHP. Zero/one/three transfers check scaling. Two further
fixtures cover a borrowed property-string upload result and a throwing callback.

The heap test first exposed the documented per-invocation argument/result leak:
18 live blocks / 800 bytes after three transfers. Scoped cleanup now retains a
borrowed result before releasing generated arguments, then releases that result
after copying its integer or upload bytes. Partial argument construction and
callback failures retire prior owners. A remaining six blocks / 264 bytes came
from curl_init URL normalization; its temporary string and direct-expression
argument are also retired, including bridge allocation failure.

Final focused evidence: both new integration tests pass on/off, with five argv
fixtures per mode (ten executions), PHP-exact stdout and clean heaps. Fifteen curl
unit tests pass, including a failed partial frame setup on one/two handles and
URL cleanup when the bridge cannot allocate. Removing frame rollback makes the
new partial-setup unit regression fail; restoring it passes. The initial skipped
native run and failing heap observations remain in the temporary evidence and
are not counted as passing tests. Managed native curl was installed into a
separate temporary project; no native manifest was added to this worktree.

Evidence directory: `/private/tmp/elephc-1840-review`. The source is shared Rust;
Linux and supported-target matrix execution for this follow-up remains a CI gate.
The original pre-existing iOS pcntl blockers are still not a passing local gate.

The final curl-feature dev build is warning-free. Three existing upload, caught
callback-throw and nested-transfer controls pass after the last production edit.
Builtin export/render, module sections, PHP comparison, catalog/site checks and
the EIR architecture audit pass; generated registries/pages have no changes.
Diff hygiene and all touched Rust module preambles pass. No full suite was run.
