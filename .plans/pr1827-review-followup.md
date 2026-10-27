# PR1827 enum trait-constant review follow-up

- [x] Pin current fork head and read all unresolved threads plus the human review.
- [x] Reproduce computed scope, collisions, magic-constant panic, deferred parent reads and private/protected exposure against PHP and the exact current compiler.
- [x] Preserve the detached earlier correction and the existing branch; rebase on current main.
- [x] Restore scope/collision protections and implement the remaining reviewed semantics.
- [x] Add focused normal/error/optimizer regression tests and validate exact values and heap cleanup.
- [x] Check supported target lowering, source hygiene and Trail findings (lexical matches explained in the commit).
- [ ] Squash the feature, fresh-rebase and push the existing fork PR1827 branch; inspect exact-head CI.

Reviewed head: d21c825487a4a4d1a849b5254cf77c8616f6458c.
Current main at start: 40c2d2ab3cf96a54e73b9691c8e83075b7eefc72.
Earlier reviewed commit8bd617c1 is preserved as preserve/1827-reviewed-8bd617c1.
All code, worktrees, temporary files and evidence remain inside Elephc.
No public review reply or thread resolution is authorized or implied.

## Final local evidence

- Candidate2 root compiler builds warning-free. The four strict materialization/access sources cover16 fresh argv variants per IR mode;32 native CLI cases match PHP and require clean heaps. Separate instance/closure controls keep their exact output assertions without a heap-clean claim.
- Twelve selected standard-harness tests pass per IR mode, including the original imported-constant/reflection-order regression, restored lexical/case-sensitive tests and separate Reflection controls.
- Fifty program/runtime assembly pairs cover all five supported targets, both IR modes and five fixture sources.102 fresh macOS ARM64/Linux ARM64/Linux x86_64 executions match complete PHP output and leave clean heaps. iOS device/Simulator evidence is assembly only; explicit probe functions retain the executable witness bodies.
- Four restored Rust diagnostics pass;20 exact final-CLI target-specific diagnostic checks pass. The example matches PHP with clean heaps in both IR modes.
- A 40-byte enum-case receiver remains retained by both reviewed d21 and final compiler, even without a closure and with an explicit local/unset. The witness is separate from constant-value ownership; preserve receiver-controls-valid.json and closure-control.json. The instance/closure controls remain functional. No original test was weakened.
- Invalid/incomplete attempts are retained: candidate1 NeverFunctionReturns, missing library snapshot, the old runtime probe missing magic lowering, and the wrong iOS diagnostic output mode. They are not final acceptance evidence.

## Integration/publication

The newly published RustSec advisories affect unchanged main dependencies. Rebase the single feature commit above the independently validated security fix922730f1(PR1954), check the combined compiler, then fresh-fetch/rebase main before updating the same fork branch with an exact remote lease. Native evidence above predates that independent dependency refresh; exact-new-head CI remains required. Keep ready PR1827 ready, and do not post replies or resolve review threads without authorization.

TLS dependency integration: warning-free `cargo check --locked -p elephc`; freshly fetched advisory database reports zero vulnerabilities with unchanged allowed-warning policy. All PHP production file digests still match the validated candidate2 snapshot.

## Main advanced during the final rebase

Main a8f730c5 merged PR1674 after candidate2 validation. Preserve its enum interface/reference-return paths, expanded schema signatures, dedicated constant walker and trait-constant binding API. The rebased feature drops the temporary walker opt-in/signature extension and duplicate metadata plumbing; retain only lazy parent normalization, shared visibility checks, throwing effects and case-collision validation. Candidate3 must revalidate the integrated source before publication; candidate2 remains historical evidence.

## Candidate4 final integrated acceptance on main a8f730c5

- Warning-free final compiler build; nine matching bridge archives frozen from the integrated/security-updated workspace.
-23 standard-harness tests pass in each IR mode: the12 PR controls plus11 focused upstream enum regressions (trait requirements/magic metadata, interface/reference/default and builtin-factory contracts).
-32 strict fresh CLI materialization/access cases pass; functional receiver controls stay separate.50 supported-target program/runtime pairs assemble and102 fresh native macOS/Linux executions match PHP with clean heaps.20 target diagnostic checks and examplebothIR pass.
- All20 iOS witness assemblies contain real probe labels; no iOS runtime execution is claimed. The40-byte receiver retention is rechecked on candidate4 and matches reviewed baseline.
- The final branch does not alter main's dedicated constant walkers, trait binding API or driver/schema argument order. It adds collision validation, read-time visibility, lazy parent normalization and conservative throwing effects.
