# Windows PE finalization and acceptance

This is the continuation ledger for PR #446, not a claim of complete PHP parity.
Keep Windows experimental until every acceptance gate below is satisfied on one
published revision. Focused Wine checks, native PHP probes, and compile-only
checks are separate evidence, not substitutes for native Windows CI.

## Integration invariants

- Generated Windows calls into Rust use the MS x64 C ABI planner, including
  shadow space and overflow arguments. Shared runtime helpers retain their
  internal convention; only genuine Rust-to-generated callbacks translate it.
- PHP registration availability belongs to the dependency-neutral builtin
  contract and is independent from backend implementation support.
- OPcache borrowed strings use explicit caller-owned C output records. Preserve
  thread-local byte storage and copy before its next reuse.
- Preserve balanced destructor frames and both float-format entry symbols on
  every target, not just Windows.
- Timezone offset resolution must use the same PHP/timelib database version as
  the embedded introspection data, including explicit future transitions,
  POSIX continuation rules, aliases, and fixed abbreviation zones. Never freeze
  the final 2037 offset or silently substitute the host CRT timezone database.
- Preserve upstream changes during rebase. Regenerate generated documentation
  after semantic conflict resolution; do not choose a stale generated side.

## Current repair batch

- [x] Correct Windows CRT localtime/access APIs and locale categories.
- [x] Restore float-format symbols used by newer main consumers.
- [x] Repair the stale linker-test target argument and redundant eval arity override.
- [x] Correct outbound eval scope adapters and target-native OPcache calls.
- [x] Centralize PHP registration availability for Unix link ownership and Windows SAPI.
- [x] Provide wide civil-time conversion and RFC 2822 mail-date formatting.
- [x] Complete PHP/timelib runtime timezone continuation beyond the public transition projection.
- [x] Execute fresh default-heap opaque-eval, mbstring, backtrace/output and SMTP PE probes under Wine (pre-rebase).
- [ ] Repeat affected PE, OPcache, float and destructor regressions on the integrated artifact.
- [x] Regenerate and audit builtin documentation and target metadata (local pre-rebase snapshot; repeat after integration).
- [x] Squash and rebase onto main `748bf44eef8597ad7b63642052b7a8dc87f5d863`, resolve semantic conflicts and regenerate documentation.
- [x] Resolve the subsequent rebase onto main `6a23959585be4534272ca54a278f7d299d2ab4b0` (new `preg_match_all` captures).
- [x] Validate the new regex native-call ABI and PHP semantic repair checkpoint, with authentic PCRE2 10.47/r5 artifacts.
- [x] Add a Windows production PCRE2 source-build CI gate (isolated cache, locked offline reuse, exact receipt/checksums, every COFF member, shim symbols and final PE link).
- [ ] Observe the new source-build gate passing in CI; the MSYS2-aligned test provider alone is not production recipe evidence.
- [x] Repeat focused Windows/eval/autoload/mbstring checks and workspace Windows GNU compilation after integration.
- [x] Publish the repair checkpoints and update the draft PR with exact provenance/results (final integrated publication remains required).
- [ ] Observe and repair the fresh complete CI matrix, including PDO runtime execution.

## Further audit findings to reconcile after rebase

The subsequent main integration introduced capture/count PCRE boundaries that
must use Windows CRT shims and the native MS x64 bridge planner. Structural
checks passed before the semantic follow-up; the first authentic-production
PCRE2 10.47/r5 PE replay exited successfully with clean heap but failed exact
PHP output on SET_ORDER trailing unmatched groups and invalid flags. That is
failed parity evidence, not acceptance.

The bounded repair batch now passes 179 Windows-focused compiler units,
10 six-target regex/ABI units, 9 executable regex regressions (including clean
heap and alias checks), 4 eval units, the full generated-doc audits, and the
workspace Windows GNU library check. The broader local PE compile-test pass had
102 passes and 6 link failures without the isolated SDK environment. Those six
focused regressions pass after exporting the isolated MinGW sysroot and adding
verified MSYS2 zlib/bzip2 COFF archives; all existing SDK files were preserved.
This mixed host/cross-test result is not native Windows CI acceptance.

Three fresh production-cache PE probes pass at the default 8 MiB heap, with
exact PHP Windows 8.5.6 output and zero live blocks. The genuine PCRE2 10.47/r5
source-built receipt remains unchanged; these are not MSYS2 test-provider receipts.

| Probe | PE SHA-256 | Heap allocation/free counts |
| --- | --- | --- |
| Eight capture/flags/writeback cases | `c01c4b6980dcd5bbdea3e88a4509e9a4d4ac1eb124ae9b6775517acba2932e75` | 232 / 232 |
| Six flag/error/output cases | `3fb9d0527c97f40632814140270afc73b55f2335cb046a30af2630bf3287e7c9` | 135 / 135 |
| Count-only, two arguments | `99140a675b97a203680f38f7be4a629f4e029fb28ad7d8f804e046d6cc4dff33` | 0 / 0 |

Compiler SHA-256: `dcd67255e1828251d5beea94a3dab8b8287abe36c136057cfb2fdd8c2fe9421b`.
Evidence is retained under `/tmp/pr446-preg-match-all-candidate-final-*`.

The current php-src audit and native PHP Windows 8.5.6 oracle confirm that
invalid low-byte order flags clear supplied matches before ValueError, while
unknown high flag bits are ignored. Count-only omitted-matches exceptions must
retire the planner's owned default ArrayNew, not leak one array per call.

Additional shared PHP-parity gaps remain distinct from Windows ABI repairs:

- Transport capability reporting must reflect linked TLS support and preserve
  PHP registration order. Native PHP Windows 8.5.6 reports `tcp,udp` with `-n`,
  and `tcp,udp,ssl,tls,tlsv1.0,tlsv1.1,tlsv1.2,tlsv1.3` with its pinned OpenSSL
  module loaded. Elephc's lowerer currently reports TLS even without requiring
  its bridge, and lists tls before ssl. Compare matching extension/capability
  profiles; absence from `-n` alone is not a blanket unsupported-function bug.
  Unix/udg absence is correct on Windows: php-src explicitly excludes AF_UNIX.

- PHP compiles the regex before validating flags; an invalid regex leaves an
  existing matches destination unchanged and returns false. The current guard
  order and integer-only return contract do not implement this behavior fully.
- PHP accepts a fifth offset argument. Elephc currently has only four parameters.
  Negative offsets count from the end (too-negative clamps to zero); an offset
  past the end returns false after clearing matches; PHP_INT_MIN raises before
  clearing matches but only after regex compilation. These need shared AOT/eval
  contract, lowering, runtime and oracle regressions, not a Windows-only patch.

Source: `php/php-src` tag `php-8.5.10`, `ext/pcre/php_pcre.c:1035-1065,
1108-1150`. This audit does not establish complete PCRE/PHP compliance.

The integrated checks pass: 176 Windows-focused compiler tests, 35 eval bridge
tests (including explicit multi-target assembly checks), 7 autoload tests and
8 mbstring variable-ownership tests. All workspace libraries cross-check for
Windows GNU. Rebase follow-ups preserve newly added generic AST fields and
check callback target availability before reading builtin signatures, while
retaining user-defined functions with names absent from PHP on Windows.

The global `Build & Test` CI verdict now requires Windows cross/LLVM/native
bridge jobs and the strict native-codegen gate. All Windows jobs use main's
Rust toolchain and GitHub Action generations; the eight-shard coverage and
14 MSVC / 16 GNU bridge inventories remain unchanged. Full CI is still required.

Fresh default-heap opaque eval executes successfully under Wine after the
scope-writeback and nested-callback repairs. All four current PE probes use the
default 8 MiB heap, exit 0 and have empty stderr:

| Probe | SHA-256 | Exact stdout |
| --- | --- | --- |
| Minimal eval | `d3323f70a69e013743353140e6ba68863e4f2519f7d17b7e99cc8c81b11df843` | `1` |
| mbconvert writeback | `c9645c6fa68698c7a063578df58789cb8f753b4e9d5242c5d452fe790e8341e4` | `ISO-8859-1\|636166c3a9` |
| Native-frame backtrace | `f6153333b904d1b558b72841590e8fd291770e158b1a522ebe1799ddc4dec66e` | `sentinel:ignored` |
| Builtin/output callbacks | `f6708a702145659171f11641bf987f19096234bd97ed3d2ad1e89a78c159c010` | `T:A:3:4` |

These are fresh post-rebase probes. They are not complete native Windows acceptance. Earlier probes
crashed during minimal eval or reported mbstring heap exhaustion. The tested executable contains
the R11 guard and release callback corrections. A speculative heap-free spill was
reverted: the identity-retirement helper already preserves RAX and all GPR/XMM.
Live QEMU/LLDB evidence subsequently located a generated scope-writeback caller
staging RCX/RDX/R8 into internal SysV `__rt_cleanup_invoke`. The caller now uses
RDI/RSI/RDX, with all-target regression coverage and the minimal replay above.
Live allocation tracing found a separate double-adapter in the literal-string
callback: its normalized internal arguments re-entered the public string C
adapter, turning a payload pointer into an allocation length. The bounded fix
calls the internal constructor; its all-target regression and fresh mbstring PE
replay pass. The generated dispatcher/output callback family is corrected too,
while retaining the public ABI for actual Rust exports; complete focused
eval-bridge tests pass 35/35 and the opaque cast/math/output scenario passes on
macOS with PHP cross-checking and under Wine as recorded above.
Structural regressions are not acceptance. The separate compiled AOT/eval SMTP
two-delivery gate now passes under Wine: PE SHA-256
`ebc9e9bceb2222f2aa8002bf2ac7b4f8b372d39da8d08b6cb7af5e08ebd86d33`, exit 0,
stdout `11`, empty stderr; the peer exited 0 after checking Date offsets +0200
and +0300 and no Bcc headers. This probe was repeated successfully after integration.

The backtrace callback now has its exact two-argument C entry adapter and uses
the internal runtime convention after normalization. Its assembly regression and
opaque native-frame/options executable regression pass on macOS with PHP
cross-checking; its fresh Wine execution passes as recorded above. Native Windows
CI remains a separate gate.

The wide Magician date representation does not widen the shared AOT calendar ABI.
Current AOT consumers still load `struct tm.tm_year` as a 32-bit word. Windows
400-year shifting avoids ordinary CRT range failures, but cannot represent the
approximately 292-billion-year values at PHP's signed timestamp endpoints.
This is a cross-target AOT limitation and requires a shared wide representation,
not a Windows-only workaround or an acceptance-test exemption.

The AOT `date`, `gmdate`, `getdate`, and `localtime` helpers also use `-1` for
an omitted/current timestamp, conflating it with PHP's valid explicit pre-epoch
timestamp. Add native value regressions for explicit `-1`, ordinary pre-1970,
expanded years and the signed endpoints, preserving omitted/null semantics.
The bounded sentinel repair must carry a separate `use_current` bit through the
typed EIR/runtime arguments: omitted or null means current, while every explicit
integer (including -1) remains an exact timestamp. Dynamic nullable boxed inputs
must be evaluated once and tested for null before integer coercion. Review the
legacy localtime contract default, named arguments and first-class callables;
keep this correction separate from widening the AOT calendar representation.

Audit deliberately rejected Windows unwind regions against native unwind/crash
probes. Conservative emission and assembly decoding alone do not prove complete
runtime unwind support.

## Required acceptance before promotion

1. Every runnable Windows `ci` codegen fixture executes exactly once across all
   eight native shards, with no baseline allowlist, skips disguised as passes,
   missing reports, duplicates, or failures.
2. Native PE, web, eval, and bridge tests pass. Both GNU/COFF production archives
   and the intended MSVC host-facing test surface are verified, including
   Unicode paths, library export checks, LLVM/LLD, and external dependency closure.
3. Relevant PHP Windows behavior is compared to a pinned native PHP oracle and
   php-src provenance: SAPI/function visibility, filesystem paths/access,
   process execution, sockets, timezones, INI, mail, and runtime ownership/ABI.
   Record unsupported PHP-on-Windows functions as absence, not fake stubs.
4. Existing macOS/Linux executable lanes and iOS compile/emitter lanes remain
   green on the same revision. A Windows repair must not reduce those targets.
5. Complete the target-policy and remaining-gap audit; synchronize developer
   policy, user documentation, packaging and CI before promotion. Do not infer
   100% compliance from a small regression subset or from model agreement.

## Oracle provenance

The existing timezone dataset is PHP 8.5.6 / tzdb 2026.1. The matching php-src
tag object is `b208d6046a61da2f2dfdd45883ce02d5299d1b22`, resolving to commit
`fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633`. Runtime continuation metadata must
be extracted reproducibly from that database, not from local PHP 8.5.10 / tzdb
2026.3. A future database update is a separate, explicit full regeneration.

Native PHP Windows oracles are isolated test artifacts with app-local Microsoft
runtime DLLs; they do not change the compiler's runtime-dependency contract.
