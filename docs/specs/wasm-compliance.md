---
title: "WebAssembly and PHP Compliance"
description: "Normative implementation and acceptance specification for the wasm32-wasi target."
sidebar:
  order: 13
---

This document is the acceptance contract for Elephc's `wasm32-wasi` target. It
replaces feature-count claims with externally observable requirements. A phase,
test count, or generated artifact is not evidence of compliance unless every
applicable acceptance gate in this document passes.

## Evidence status

This table distinguishes implementation progress from acceptance. **Satisfied**
means that the requirement has durable, automated evidence in the repository.
**Partially satisfied** means that an implementation or a local observation
exists but at least one acceptance gate remains open. **Open** means that the
available evidence is insufficient.

| Area | Status | Durable evidence and remaining work |
|---|---|---|
| Normative contract | Partially satisfied | Core 3.0 and WASI Preview 1 are identified, with the WASI ABI pinned below. Every PHP acceptance run must still record immutable php-src revisions and the complete oracle environment. |
| Independent implementation audits | Partially satisfied | GLM 5.2, Kimi K2.7, and MiniMax M3 independently audited Elephc `b3b399408c9fabcb7b32a428e09d4b7fca09e320` through dedicated Codex instances and accepted the same canonical 24-item registry after four convergence rounds. This establishes the implementation baseline, not approval of the future final revision. |
| Initial baseline | Partially satisfied | Counts and corpus results were recorded at one compiler revision. They are historical observations, not a generated current-revision coverage report. |
| Production validation and artifact integrity | Partially satisfied | Production in-process assembly/Core validation, external validation in the shared host job, and transaction-safe WAT/WASM/npm publication exist. Exhaustive invalid-input and rollback-path coverage remains open. |
| Capability audit | Partially satisfied | Every EIR identity is classified, cross-cutting transfer/call/branch/local/global/object/property/iterator shape checks run before emission, and acceptance returns an exact fully lowered plan rather than re-running lowering during publication. Nullable `ArrayGet` and `HashGet` shapes preserve exact `container|null` storage; direct calls on exact `Object|null` pointers require a dominating false edge of `IsNull(receiver)`. Dynamic properties are readable only after a same-block write proof or through a statically null nullsafe receiver. The only admitted static `Warn` and `ThrowError` forms are the exact array-offset-on-null and method-on-null boundaries in a main-bearing command module. Dynamic `Mixed` or float keys, scalar float-to-int conversions, generic Mixed-to-scalar conversions, unproved iterator storage/mutation, unsupported globals, and unproved property layouts fail before WAT generation. The remaining gap is exhaustive generated PHP-source reachability evidence, not post-acceptance lowering. |
| Core trap inventory | Partially satisfied | Production generation lexes the final WAT, requires a marker for every real `unreachable`, assembles the artifact, and proves the marker count equals the final Core operator count. Post-noreturn helper proofs must be final in the same helper body and reject Core 3.0 returns, tail calls, structured branches, typed-reference branches, and exception handlers that could bypass the final trap. OOM, PHP-visible, and explicitly non-public reactor-only sites are closed classes, and non-public sites/reactor modules are rejected at the public command boundary. Exact-revision CI and final independent review remain open. |
| Typed transfer and control flow | Partially satisfied | The canonical storage-pair matrix, local/block/call/return transfer guards, and focused branch regressions exist. Exhaustive PHP-source reachability, switch, ownership, and cross-host matrices remain open. |
| WASI startup, arguments/environment, I/O, and process status | Partially satisfied | The pinned three-host job proves exact `$argc`/`$argv`, stdout/stderr, `exit(7)`, repeated Node runs, and partial-`fd_write` progress. Environment/preopen mapping, complete byte contracts, and the full status boundary remain open. |
| Numeric and PHP error semantics | Partially satisfied | Admitted indexed int/bool/string and associative int/bool/string-keyed misses preserve null and emit ordered key-class-specific stderr warnings on normal reads across the 8.2–8.5 compiler profiles; silent reads omit the warning. Float values remain valid associative elements, but float keys and scalar float-to-int conversions are rejected until their versioned diagnostics are implemented. A chained read through an intermediate null still evaluates its later index exactly once; normal reads then emit the profile-exact PHP array-offset-on-null warning (`on value of type null` for 8.2, `on null` for 8.3–8.5), while silent reads suppress diagnostics without suppressing index side effects. A method call through a missing object-valued associative key emits the warning, terminates with status 255 and the PHP method-on-null message, and does not evaluate later arguments. The former integer sentinel remains a valid in-range value. Source locations, stack traces, exceptional cleanup, and the complete versioned php-src differential and diagnostics matrices remain open. |
| Allocator, ownership, COW, and adversarial safety | Partially satisfied | Focused implementation and cleanup tests exist. Boolean indexed arrays stamp their runtime element tag before hash promotion; scalar `mixed` property defaults are boxed; owned cells transfer while borrowed/local-loaded cells are retained independently; and `usort` or direct container mutation during a live `foreach` fails closed. Exhaustive resource, malformed-state, aliasing, and failure-path evidence remains open. |
| Deterministic artifacts | Partially satisfied | Deterministic IDs have focused tests, and the pinned CI gate compares WAT, WASM, npm trees, and packed archives from independent compiler processes. Relevant map insertion orders and the complete metadata-normalization contract remain open. |
| PHP differential parity | Open | No committed full-version oracle matrix covers the declared reachable surface. |
| Wasmer, Wasmtime, Node, and external-validator CI | Satisfied | CI run `30439384471` on Elephc `b3b399408c9` validates and executes one artifact under checksum-pinned Wasmer 7.2.1, Wasmtime 47.0.2, `wasm-tools` 1.254.0, Node 26.3.0, and TypeScript 6.0.3. |
| Executable npm package matrix | Partially satisfied | The pinned job proves direct execution, repeated imported `run()`, exact status/output, package contents, reproducible archives, and strict NodeNext declarations. Environment, preopen, thrown-host-error, and concurrent-instance cases remain open. |
| Final exact-revision review | Open | Three independent reviewers must inspect the same final source revision and its recorded acceptance logs, with no unresolved blocker. |

### Audit provenance

The baseline audit used the requested `glm-5.2:cloud`,
`kimi-k2.7-code:cloud`, and `minimax-m3:cloud` Ollama models. Each model ran
through a dedicated Codex instance with filesystem access and inspected the
same clean Elephc revision,
`b3b399408c9fabcb7b32a428e09d4b7fca09e320`. The reviewers first produced
independent findings, then voted on the same evidence-backed registry. A fourth
round removed the remaining classification ambiguity by applying one closed
taxonomy. All three accepted every canonical entry, its acceptance contract,
and the implementation order without replacement.

That consensus fixes the implementation baseline only. It is not transferable
to a later commit: the final revision, evidence manifest, and logs still require
three independent exact-revision approvals.

### Finalization audit checkpoint

The first finalization audit inspected clean Elephc revision
`c61ab588c6c860337f10abd8949980d36c267dcf` and specification SHA-256
`270a956fea81e861d56bbafb804c0c87d78d8092f9fb6c3db8860f3a27ffa251`.
GLM 5.2, Kimi K2.7, and MiniMax M3 independently returned **BLOCK**. This is
unanimous evidence that the revision is not merge-ready; it is not an approval
of every raw finding. The normalized implementation ledger below retains only
findings that are supported by the effective code, the public contract, or a
missing durable gate.

The normalization explicitly rejects these audit false positives:

- npm argument validation is behaviorally exercised: the pinned Node job runs
  embedded-NUL and unpaired-surrogate inputs and checks the typed error before
  WASI construction;
- the roadmap checkboxes describe delivered experimental artifacts, while the
  roadmap, changelog, README, target guide, and CLI still label incomplete
  parity truthfully;
- `Immediate::CastTarget` and `Immediate::ExplicitCastTarget` are both covered
  by the EIR validator, printer, native backend, and WASM backend;
- the two production `unreachable!` sites audited in the Rust backend are
  unreachable by their validated representation partitions; generated Core
  `unreachable` instructions remain governed separately by R1 and R2;
- the dedicated host job and complete native CI were green for the checkpoint,
  but that evidence is not transferable to later implementation or squash
  revisions.

The remaining blockers are substantive: valid PHP remains rejected in several
families, no complete php-src differential harness or generated current-revision
coverage report exists, exceptional PHP behavior is not generally catchable,
and the WASI process/environment boundary is incomplete.

### Canonical audit taxonomy

The consensus registry uses the following closed classes:

- **MERGE-BLOCKER-PROVEN**: a currently reachable public behavior is wrong, or
  an admitted EIR shape directly violates the pre-emission capability
  invariant;
- **LATENT-BACKEND-GATE**: a real backend defect is currently shielded by a
  pre-emission rejection of the PHP reproducer;
- **FULL-COMPLIANCE-GATE**: valid PHP remains unsupported, or an exhaustive
  versioned matrix is required, without a proven currently admitted wrong
  behavior;
- **HARDENING**: malformed or adversarial runtime state has no demonstrated
  valid-PHP reachability;
- **DOC/CI**: a public contract, documentation claim, artifact result, or CI
  supply-chain requirement is wrong or incomplete.

Severity measures the impact of one entry, not the overall size of the
remaining campaign.

### Canonical audit registry

| ID | Stable requirement | Severity | Class | Required result |
|---|---|---|---|---|
| A | `WASM-OWN-001` escaping by-reference closure cells | Critical | MERGE-BLOCKER-PROVEN | The escaping-closure regression prints `23`; a captured cell is not freed by its creator's epilogue and is released exactly once after its final closure/local owner. |
| B | `WASM-HASH-001` tombstone-aware insertion | High | LATENT-BACKEND-GATE | Probing is bounded, reuses the first tombstone, and a filled-then-fully-deleted table accepts reinsertion without hanging. |
| C | `WASM-MEM-004` allocation-size narrowing | High | LATENT-BACKEND-GATE | Every capacity/length multiplication and `i64` to `i32` narrowing is checked before mutation; wasm32 boundary tests reach deterministic OOM/diagnostics rather than wrapped allocation. |
| D1 | `WASM-NPM-002` JavaScript argument encoding | High | MERGE-BLOCKER-PROVEN | Embedded NUL and unpaired UTF-16 surrogates are rejected with a typed package error before WASI construction; no silent truncation or replacement is permitted. |
| D2 | `WASM-ABI-003` environment and preopen boundary | Medium | FULL-COMPLIANCE-GATE | One documented mapping covers NUL, `=`, Unicode encoding, ordering, missing/empty values, duplicate limitations, path aliases, and repeated runs. |
| E | `PHP-WASM-CAST-001` complete Mixed casts | High | FULL-COMPLIANCE-GATE | A generated source-tag by destination-type matrix matches pinned php-src profiles for values, warnings, errors, and ownership. |
| F | `PHP-WASM-ARRAY-001` array/hash read failures | Medium | LATENT-BACKEND-GATE | Missing indexed or associative keys use PHP warning/null behavior; integer and string diagnostics retain their PHP key formatting, and internal sentinels never escape once the shape is admitted. |
| G | `PHP-WASM-ERROR-001` invalid dynamic operations | Critical | MERGE-BLOCKER-PROVEN | Invalid operations such as a method call on an integer produce the pinned PHP `Error`, message, cleanup, stderr, and status, never a raw Core trap. |
| H | `PHP-WASM-NUM-004` float-to-int conversion | High | MERGE-BLOCKER-PROVEN | Boundary, overflow, `NaN`, and infinity conversions match each pinned php-src profile; `(int) 1.0e20` is a mandatory regression. |
| I | `WASM-HASH-002` PHP next-free key | Medium | LATENT-BACKEND-GATE | Hashes persist PHP's `nNextFreeElement` semantics across deletion, COW, clone, resize, explicit keys, and `PHP_INT_MAX`. |
| J | `WASI-EXIT-001` complete PHP exit semantics | High | FULL-COMPLIANCE-GATE | Versioned scalar coercion/output, top-level return, exit outside main, cleanup/unwind, and the WASI i32 status boundary match pinned php-src and all host APIs. |
| K | `WASM-ART-002` shape-complete capability audit | High | MERGE-BLOCKER-PROVEN | Every admitted shape is lowerable; every other shape is rejected before staging. No accepted module reaches a late backend `Unsupported`. |
| L1 | `WASM-CALL-001` admitted callable shapes | High | MERGE-BLOCKER-PROVEN | All currently admitted call/method/closure/FCC shapes pass the capability-to-lowerer parity gate and positive execution corpus. |
| L2 | `WASM-CALL-002` remaining PHP callables | High | FULL-COMPLIANCE-GATE | Valid PHP named/spread/variadic/by-ref/method/closure/FCC forms execute with PHP-equivalent evaluation, ABI, writeback, errors, and ownership. |
| M1 | `WASM-MEM-005` malformed Mixed chains | Medium | HARDENING | Nested/cyclic/malformed Mixed chains are bounded and fail deterministically without unbounded loops or unchecked loads. |
| M2 | `WASM-MEM-006` heap-pointer provenance | Medium | HARDENING | Refcount/free/header helpers accept only aligned starts of live allocations with complete in-bounds headers and payloads; interior pointers are rejected. |
| M3 | `WASM-MEM-003` recursive PHP ownership | High | FULL-COMPLIANCE-GATE | Differential tests cover references, recursive structures, COW, capture cycles, destructors, normal/exceptional exits, and heap balance. |
| N1 | `WASM-ART-003` post-commit cleanup result | Medium | DOC/CI | Publication either rolls back on every reported failure or reports committed success with a separate cleanup warning; rustdoc, API result, and tests describe the same state. |
| N2 | `WASM-DOC-001` truthful public surface | High | DOC/CI | Changelog and public docs remove “any WASI host” and unsupported builtin claims and label the target experimental until every completion gate closes. |
| N3 | `WASM-CI-001` verified Wasmer installation | Medium | DOC/CI | Every WASM CI job installs the pinned Wasmer artifact through a checksum-verified path; no WASM job executes an unverified `curl | sh` installer. |
| O | `WASM-COVERAGE-001` generated full inventory | Global | FULL-COMPLIANCE-GATE | Current-revision reports cover `Op`, runtime targets/functions, unary runtimes, terminators, shapes, reachable PHP fixtures, exclusions, and a reproducible php-src differential matrix. |
| P | `PHP-WASM-ERROR-002` diagnostics architecture | High | FULL-COMPLIANCE-GATE | Warnings, fatals, exceptions, suppression, `error_reporting`, messages, traces, cleanup, stderr, and statuses have one versioned PHP-equivalent path. |
| R1 | `WASM-TRAP-001` reachable `unreachable` sites | Critical | MERGE-BLOCKER-PROVEN | Every currently PHP-observable `unreachable` path is replaced with PHP behavior; G is the mandatory initial regression. |
| R2 | `WASM-TRAP-002` complete `unreachable` inventory | High | FULL-COMPLIANCE-GATE | Every emitted `unreachable` is generated into an inventory and classified as post-noreturn, proven invariant, deterministic OOM, a PHP-visible path that must be implemented, or an explicitly non-public reactor-only path that production command validation rejects. |

The exact current capability inventory at the audited SHA is 90 of 236 `Op`
variants, 4 of 437 `RuntimeFnId` variants, 0 of 15
`UnaryStringRuntime` variants, and 5 of 8 `Terminator` variants. These figures
are evidence for `WASM-COVERAGE-001`, not acceptance thresholds.

### Consensus implementation order

The three reviewers accepted this dependency order:

1. close A, D1, G, H, and R1;
2. make K and L1 enforceable while generating the R2 inventory;
3. close B, C, F, and I;
4. close D2, J, L2, E, P, and M3;
5. close M1 and M2;
6. align N1, N2, and N3;
7. regenerate O and run the final exact-revision differential, host, artifact,
   ownership, and reviewer gates.

### Exact finalization implementation ledger

This ledger is normative for implementation. Each package must land with its
listed code, PHP-source, cross-host, and generated-evidence gates. A package is
not complete merely because the capability audit rejects the missing behavior.

| Package | Registry coverage | Required implementation | Required durable evidence |
|---|---|---|---|
| W0 — normative evidence and generated inventory | O, N3 | Pin every normative revision and tool version; generate the current `Op`, `RuntimeCallTarget`, `RuntimeFnId`, `UnaryStringRuntime`, terminator, shape, PHP-producer, lowerer, test, and exclusion inventory; reject stale hand-maintained counts. | A deterministic machine-readable report committed or retained by CI, a human summary generated from it, schema validation, and a test that every identity has exactly one supported or excluded disposition. |
| W1 — scalars, strings, casts, and numeric semantics | E, H, P | Implement every scalar and `Mixed` cast by source tag and destination type; implement string comparisons, strict/loose equality, spaceship, indexing, persistence, interpolation, numeric-string conversion, checked mixed arithmetic, float/Mixed truthiness, float keys, and profile-specific float-to-int values, warnings, and deprecations. | Generated PHP 8.2/8.3/8.4/8.5 oracle matrices covering boundary integers, signed zero, `NaN`, infinities, out-of-range values, binary/NUL strings, numeric strings, warning order, stderr, exit status, and owned/borrowed results. `(int) 1.0e20` is mandatory. |
| W2 — PHP diagnostics, exceptions, and Core traps | G, P, R1, R2 | Replace process-only arithmetic failures with catchable PHP `Error`/`TypeError`/`DivisionByZeroError`/`ArithmeticError` paths; implement throw/catch/finally, suppression, `error_reporting`, source locations, traces, and exceptional cleanup; keep only proven post-noreturn, invariant, deterministic-OOM, or excluded non-public Core traps. | PHP-source caught and uncaught exception matrices, method calls on every non-object tag, warning/fatal ordering, trace normalization, cleanup/heap-balance assertions, and generated trap inventory with binary operator-count parity. |
| W3 — calls, control flow, and exit/unwind | J, K, L1, L2 | Complete named, spread, variadic, by-reference, method, closure, callable-array, first-class-callable, expression-call, and function-variant shapes; preserve source evaluation and writeback; implement `exit`/`die` from every frame, top-level `return`, cleanup/unwind, and the PHP-int to WASI-i32 status contract. | Positive and negative PHP-source matrices for all call shapes, alias/writeback and ownership tests, nested-frame exit tests, top-level return tests, and Wasmer/Wasmtime/Node status checks for negative, zero, 255, 256, `i32` boundaries, and PHP-int boundaries. |
| W4 — containers, objects, globals, dynamic execution, and runtimes | B, F, I, K, O | Complete mixed array/hash keys, `isset`, append, spread, list/unpack, reference element access, COW, next-free-key behavior, dynamic construction/properties, clone, static properties, late static binding, globals/statics, supported AOT include/eval guards, generators/iterators, and every PHP-visible builtin/runtime identity admitted by the target. | Full applicable example/codegen corpus replay, php-src array/object/global/generator matrices, tombstone/saturation/COW regressions, and no reachable unsupported inventory entry. |
| W5 — memory, ownership, and adversarial safety | A, C, M1, M2, M3 | Check every allocation calculation and wasm32 narrowing; validate live allocation starts, alignment, headers, payload bounds, and nested `Mixed` depth/cycles; balance ownership across aliases, calls, exceptions, destructors, COW, cycles, early return, and exit. | Bounded OOM/overflow tests, malformed/interior-pointer tests, double-free defenses, deep/cyclic `Mixed`, closure/object cycles, destructor resurrection, COW-during-borrow, multi-megabyte strings/output, and heap-balance comparisons. |
| W6 — WASI and npm host contract | D1, D2, J, N1, N3 | Define arguments, environment, preopens, byte encoding, duplicates, missing/empty values, path aliases, repeated/concurrent runs, host exceptions, errno, partial writes, and publication cleanup in one contract; expose PHP-visible environment behavior where declared. | One validated artifact executed on Wasmer, Wasmtime, and Node; typed npm error tests; empty/NUL/Unicode/invalid cases; environment/preopen observations; partial/zero-progress I/O; repeated and concurrent instances; deterministic package/archive checks. |
| W7 — artifact, corpus, CI, and documentation closure | K, N1, N2, O | Run production assembly/Core validation and transactional publication on the complete matrix; classify every checked-in example and codegen fixture as applicable or explicitly excluded; keep public claims synchronized with generated evidence. | Full native CI, dedicated WASM portability CI, external validation, all applicable examples, machine-readable exclusions with reasons and owners, generated docs, Copilot thread audit, and an exact evidence manifest. |
| W8 — exact-revision review, publication, and squash | all | Obtain three independent same-hash preliminary LOCKs on the merge-ready unsquashed revision; fetch and rebase onto current `origin/main`; squash the branch only then; publish with an exact force-with-lease; rerun every required gate and obtain three new LOCKs on the final squashed SHA and unchanged spec hash. | Green final CI on the squashed SHA, zero unresolved PR review threads, clean worktree, final evidence manifest, and GLM/Kimi/MiniMax reports whose findings arrays are empty and whose commit/spec fields match exactly. |

#### W0 report schema

The generated coverage report must contain, at minimum:

- the Elephc commit, dirty-state flag, specification hash, generator version,
  and normative/toolchain pins;
- every EIR identity with its stable name and enum family;
- all PHP-source producers and public execution modes that can reach it;
- the capability disposition and every shape predicate;
- the exact lowerer or a machine-readable exclusion category, reason, owner,
  and removal gate;
- positive, negative, differential, ownership, and host-test identifiers;
- totals derived from the identities, never copied from this prose.

Native-only Elephc extensions (`ptr`, `buffer`, `packed`, native `extern`,
native bridge crates, and the web SAPI) may be excluded from `wasm32-wasi`
only through this report and matching CLI diagnostics. Ordinary PHP supported
by Elephc's public frontend is not excludable merely because its WASM lowerer is
missing.

#### W1 and W2 oracle contract

The differential harness must build or use the exact pinned php-src releases,
run with an explicit INI and extension set, and record stdout, stderr, status,
value/type shape, locale, time zone, architecture, and path/line normalization.
It must execute the same logical fixture and arguments through php-src and the
same validated WASM artifact. Expected output copied by hand into a Rust test
does not replace this oracle.

The diagnostic runtime must distinguish warnings, catchable throwables,
uncaught fatals, and deliberate `exit`. Calling `proc_exit(255)` directly is
valid only after PHP semantics have established that the path is an uncaught
fatal and every owned frame has been cleaned.

#### W3 exit contract

WASI Preview 1 accepts an unsigned 32-bit exit code. The harness must retain the
module-level `i32` bit pattern before any shell or host normalization. Each host
adapter must expose one documented representation, and the PHP-to-WASI mapping
must be identical across Wasmer, Wasmtime, and Node. The mapping must be derived
from pinned php-src CLI behavior and explicitly cover negative and out-of-range
PHP integers rather than relying on `i32.wrap_i64` accidentally.

#### W6 npm dispositions already closed

The implemented `WasiArgumentError` checks for embedded NUL and unpaired UTF-16
surrogates are retained as D1 evidence. They must not be removed or downgraded
while W6 adds environment, preopen, thrown-host-error, zero-progress write, and
concurrent-instance coverage.

#### Implementation and review rule

Before functional implementation begins, GLM 5.2, Kimi K2.7, and MiniMax M3
must each return `LOCK` for the exact same specification SHA-256. A specification
review returns `BLOCK` for an omitted requirement, ambiguous acceptance rule,
or contradiction; implementation status and expected future evidence are not
specification blockers. Any normative edit restarts the three-model spec vote.

After implementation begins, each code review names its exact Elephc commit and
the unchanged specification hash. Any functional or normative edit invalidates
previous implementation LOCKs. Documentation-only evidence updates invalidate
the exact-revision implementation review but do not restart the specification
vote unless they change a normative requirement.

## Normative references

The following sources define separate parts of the contract:

1. [WebAssembly Core Specification 3.0](https://webassembly.github.io/spec/core/intro/introduction.html),
   frozen at WebAssembly/spec tag `wg-3.0`, commit
   `9d36019973201a19f9c9ebb0f10828b2fe2374aa`,
   including its [validation rules](https://webassembly.github.io/spec/core/valid/index.html)
   and [numeric execution semantics](https://webassembly.github.io/spec/core/exec/numerics.html),
   is normative for module validation and Core execution.
2. The official WASI Preview 1
   [API specification](https://github.com/WebAssembly/WASI/blob/e840fe45e63b4f227a29fa87df94ab3bbe3d5efb/legacy/preview1/docs.md)
   and
   [`wasi_snapshot_preview1.witx`](https://github.com/WebAssembly/WASI/blob/e840fe45e63b4f227a29fa87df94ab3bbe3d5efb/legacy/preview1/witx/wasi_snapshot_preview1.witx)
   at WebAssembly/WASI commit
   `e840fe45e63b4f227a29fa87df94ab3bbe3d5efb` are normative for the command ABI
   used by this target. Updating that snapshot requires an explicit spec change;
   a moving branch is not reproducible evidence. Preview 2 or component-model
   support requires a separate contract.
3. Exact releases of [php-src](https://github.com/php/php-src) are normative for
   PHP-visible behavior:

   | Elephc profile | php-src tag | Annotated tag object | Peeled commit |
   |---|---|---|---|
   | `8.2` | `php-8.2.33` | `fa98f62b39a612ae88b7be5d5ea9ff9b794b454b` | `651db3ebfa622cae0c4e6b39766812efbd274ced` |
   | `8.3` | `php-8.3.33` | `a7413fbd1dd4dccda419ca473ce475f084edaadd` | `4a55da8cb580ba56887c80a42291ebc676d6fb43` |
   | `8.4` | `php-8.4.24` | `3cb6f7231aed24c4ae77a0d3ee5aeeb2b968ad30` | `fb193d3df72d1ca3b5ef58ec9e9b6ef5bdf18bef` |
   | `8.5` | `php-8.5.9` | `d6bbf3ed631eea9763a2b790653fc91b69f0af7a` | `dd6e76cce27aaa0ed9f7520648ed1081dfb6af36` |

   Both identities are normative: the tag reference must resolve to the exact
   annotated-tag object and peeling that object must resolve to the exact commit.
   The oracle checks out and executes the peeled commit in detached, clean state.
   Updating any profile requires a normative specification edit and a complete
   differential rerun. The evidence manifest records both identities and the
   build configuration.

4. The acceptance toolchain is Rust `1.95.0`, `wat` `1.252.0`,
   production `wasmparser` `0.252.0`, Wasmer `7.2.1`, Wasmtime `47.0.2`,
   `wasm-tools` `1.254.0`, Node.js `26.3.0`, and TypeScript `6.0.3`.
   Package checks use the npm version shipped with the pinned Node.js runtime.
   Updating any tool requires a complete Core, host, npm, and determinism rerun.

The [Wasmtime WASI tutorial](https://github.com/bytecodealliance/wasmtime/blob/main/docs/WASI-tutorial.md)
and [Node.js WASI API](https://nodejs.org/api/wasi.html) are informative host
integration references and host-conformance targets; they do not replace the
WASI ABI definition. Host and tool versions must be pinned in the evidence
manifest rather than inferred from these moving pages.

The Core specification defines module validity and execution. It does not
define files, arguments, standard streams, or exit behavior; those are WASI
host contracts. PHP semantics remain authoritative even where the native
Elephc backend has the same bug.

## Compliance boundary

The target is compliant only when all three layers below are simultaneously
true:

### Core WebAssembly

Every `.wat`, `.wasm`, and npm-contained module emitted by Elephc:

- assembles and validates under WebAssembly Core 3.0;
- has type-correct stack, local, block, call, result, memory, and table use;
- has deterministic, collision-free identifiers and exports;
- does not depend on engine-specific acceptance or undefined behavior.

Elephc need not generate every Core instruction. It must generate valid Core
semantics for every instruction it does generate.

### WASI command ABI

Every command module:

- imports Preview 1 functions with their exact signatures;
- checks every errno-bearing call;
- handles partial writes until completion or a defined failure;
- obtains arguments through `args_sizes_get` and `args_get`;
- preserves PHP's `$argc` and `$argv`, including the script-name element;
- terminates with a deterministic status and no callable fall-through after
  `proc_exit`;
- runs without host-specific imports on Wasmer, Wasmtime, and Node.

### PHP-to-WASM semantic parity

For every EIR construct reachable from Elephc's supported PHP subset, the WASM
target must do one of the following before it publishes an artifact:

1. emit PHP-equivalent behavior, including values, types, output, errors,
   ownership, and observable ordering; or
2. fail compilation with a precise target-capability diagnostic.

The second outcome prevents silent corruption but does not constitute 100%
target support. Final compliance requires closing all reachable capability
gaps exercised by the supported PHP surface.

### Reachable surface

The reachable surface is every EIR module that Elephc can produce from
command-mode PHP source accepted under the maintained `--php-version` profiles.
Compiler extensions or host-inapplicable features may be excluded only through
a reviewed, machine-readable WASM exclusion catalog that names the PHP/EIR
surface and gives the reason. A capability identifier admitted by the
pre-emission audit but rejected later because of operand types, immediates,
representations, ownership, callable shape, or control-flow shape remains a
reachable capability gap. The unsupported count cannot be reduced by silently
shrinking this definition.

## Initial audited baseline

The historical audit measured revision
`48b3bdf9ca0d2c19e6949a5f5e89a6055db43b24`. The figures below describe that
revision only. They are not current compliance evidence and must not be copied
forward without regenerating them from a recorded command and tool manifest.

| Surface | Total | WASM dispatch | Gap |
|---|---:|---:|---:|
| EIR `Op` variants | 236 | 90 | 146 |
| `RuntimeFnId` variants | 437 | 8 | 429 |
| `RuntimeCallTarget` variants | 3 | 1 partial | 2 |
| `UnaryStringRuntime` variants | 15 | 0 | 15 |
| `Terminator` variants | 8 | 5 | 3 |

A compile-and-validate pass over the checked-in example corpus produced:

| Outcome | Count |
|---|---:|
| Compiled | 8 |
| Valid modules | 6 |
| Invalid modules | 2 |
| Diagnosed unsupported backend surface | 176 |
| Frontend rejection | 4 |

The invalid examples were `destructor` and `string-builder`. Therefore artifact
generation, focused unit counts, and a green native matrix were insufficient
evidence of target validity at the audit baseline.

## Required implementation work

### P0 — artifact integrity

#### WASM-ART-001: validate production binaries

`emit_wasm_artifacts` must generate in memory, assemble to bytes, and validate
those bytes with a production dependency such as `wasmparser` before writing
any user-visible artifact.

Acceptance:

- malformed and type-invalid WAT are rejected by a production test;
- `--emit-asm` performs the same assembly and binary validation even when only
  `.wat` is requested;
- `.wat`, `.wasm`, npm directories, and any package archive are written only
  after successful validation;
- failed generation leaves no new or partially overwritten artifact;
- a final external `wasm-tools validate` or equivalent remains in CI as an
  implementation-independent check.

#### WASM-ART-002: capability validation before emission

Add a target-capability pass over the complete EIR module before WAT generation.
It must report all unsupported reachable operations with function and EIR
context instead of failing after earlier artifacts or partial output exist.

Acceptance:

- every `Op`, `RuntimeCallTarget`, `RuntimeFnId`, `UnaryStringRuntime`,
  `IrType`, and `Terminator` variant is classified by an exhaustive Rust match;
- the classification checks operand and result arity/types, immediates,
  representations, ownership modes, callable shapes, and control-flow shapes
  wherever those properties change support;
- a module accepted by the capability pass cannot later fail with a backend
  `Unsupported` result during lowering;
- adding a new enum variant fails compilation or the parity gate until its WASM
  capability is classified and tested;
- the parity gate derives its totals and per-variant report from the
  current-revision enums and descriptors; no literal historical count is an
  acceptance threshold;
- no backend `Unsupported` error is first discovered after output publication.

The current capability boundary exhaustively classifies identities and verifies
the cross-cutting storage pairs used by locals, globals, block arguments,
branches, direct and callable calls, returns, reference cells, arrays, hashes,
objects, properties, iterators, and the admitted runtime forms. Static checks
are followed by exact in-memory lowering into the immutable plan returned to
publication, so a successful audit cannot encounter a later backend
`Unsupported`.
Admitted indexed int/bool/string and associative int/bool/string-keyed/container
reads distinguish warning-producing and silent misses while preserving null,
require ownership consistent with fresh Mixed cells or retained concrete
container pointers, and conservatively carry the EIR heap-allocation effect.
Associative container hits and misses use exact `container|null` metadata so
typed chained consumers retain their receiver representation. Indexed/hash
chained reads evaluate each index exactly once in PHP source order, then branch
on `IsNull` before the typed operation; a normal null edge emits the exact
array-offset-on-null warning while coalescing edges stay silent without
skipping index side effects. Capability validation requires the non-null edge
to dominate the exact pointer consumer. The null method-call edge is admitted only for the
exact static `Call to a member function ...() on null` error in a public command
module, and terminates through the registered PHP fatal helper before any
method argument is evaluated. General `ThrowError`, `try`/`catch`, reactor
diagnostics, source locations, traces, and exceptional cleanup remain rejected
or tracked by `PHP-WASM-ERROR-002`. Dynamic `Mixed` and float associative keys are
rejected before WAT generation for `HashGet`, `HashGetSilent`, `HashSet`, and
`HashUnset`: silent reads suppress only undefined-key warnings, not invalid-key
type errors, deprecations, or conversion warnings. EIR records source-level PHP
casts separately from implicit coercions through `ExplicitCastTarget`. Exact
boolean/string indexed-read casts, including their nullable miss path, are
admitted only when the declared element tag proves the runtime conversion.
Scalar `FToI`, diagnostic-sensitive float-to-int casts, generic
Mixed-to-scalar casts, and implicit narrowing remain rejected until their
context-, tag-, and profile-specific behavior is implemented.
For the WASM target, diagnostic-sensitive AST normalization, constant-control
pruning, dead-code elimination, and EIR optimization are disabled until after
this boundary. Constant folding and propagation retain every `NAN` truthiness
operation because PHP 8.5 reports each coercion.
Associative warnings format normalized integer keys without quotes
and string keys with quotes. Dynamic
method candidates include every concrete class exposing the method; mismatched
arity or non-public visibility is rejected by capability validation instead of
being silently removed from the runtime ladder. Complete diagnostic metadata
and suppression remain part of `PHP-WASM-ERROR-002`; PHP errors for dynamic
nullsafe receivers outside the proven closed class set remain a runtime
semantic gate. Core traps are not substitutes.

### P0 — representation and control-flow validity

#### WASM-REP-001: typed value transfer

Create one authoritative transfer/materialization layer for SSA values, local
slots, block parameters, call arguments, call results, and returns. Comparing
component counts is insufficient: the layer must compare concrete WebAssembly
types and apply explicit PHP/EIR conversions.

It must cover:

- concrete `I64`, `F64`, strings, tagged scalars, and every heap kind;
- boxing concrete values into `Mixed`/union slots;
- safe unboxing or target diagnostics in the reverse direction;
- ownership transfer versus borrow, including strings and heap objects;
- multi-value strings and returns;
- block-argument transfers and dispatch-loop merges.

Acceptance:

- the real `while ($i < 3) { $i++; }` regression validates and prints `3`;
- `examples/string-builder` validates and matches PHP;
- a generated test matrix covers every source/destination representation pair;
- Wasmer, Wasmtime, Node, and `wasmparser` agree on validity.

#### WASM-REP-002: void call results

Mirror the native `store_call_result` contract. When EIR assigns the result of a
void callee to a result slot, materialize Elephc's null sentinel and box it when
the destination is `Mixed`; never emit `local.set` against an empty WASM stack.

Acceptance:

- `examples/destructor` validates and matches native/PHP output;
- tests cover ignored void results, concrete result slots, and mixed result
  slots.

#### WASM-CFG-001: typed branches and switches

Validate every branch argument and switch scrutinee against its target
representation. Do not assume an `i64` scrutinee.

Acceptance:

- branch and switch matrices cover scalar, string, tagged, mixed, and heap
  values where EIR permits them;
- invalid EIR is rejected with an EIR diagnostic rather than emitted.

### P0 — entry point, symbols, PHP arguments, and environment

#### WASM-ABI-001: initialize `$argc` and `$argv`

The `_entry` prologue must initialize source locals corresponding to `$argc` and
`$argv` from `__rt_argc` and `__rt_argv`, using the typed-transfer layer when
their slots have widened. The harness passes the same logical argument vector
to every host: `$argc` is its element count, and `$argv[0]` is the exact program
name supplied by the harness rather than a host-selected path.

WASI arguments are byte strings. The package API must document how JavaScript
strings are encoded, reject values that the Preview 1 boundary cannot represent
(including embedded NUL), and preserve all other bytes without host-specific
rewriting.

Acceptance:

- `echo $argc` observes the host argument count including `argv[0]`;
- `$argv[0]`, numeric indexing, `foreach`, empty arguments, non-ASCII arguments,
  rejected embedded NUL, and repeated Node `run()` calls match the declared
  PHP/WASI boundary;
- tests execute on all three hosts with bounded output and timeouts.

#### WASM-ABI-002: injective symbol encoding

Replace lossy punctuation-to-underscore mangling with a deterministic injective
encoding or stable numeric symbol table. Apply it consistently to definitions,
calls, exports, table entries, dispatch stubs, methods, closures, and generated
runtime names.

Acceptance:

- `A\B()` and `A_B()` coexist;
- tests cover namespace separators, method separators, underscores, ASCII
  punctuation, Unicode identifiers, and stable output across two builds;
- definitions, references, exports, table entries, locals, labels, data
  segments, and generated runtime symbols are checked in every relevant WAT
  index space;
- duplicate identifier and duplicate export rejection remain defensive builder
  invariants.

#### WASM-ABI-003: define the environment boundary

WASI environment entries are byte strings. Direct-host and npm harnesses must
apply one documented mapping for key ordering, duplicate keys,
absence-versus-empty values, non-ASCII bytes, JavaScript string encoding, and
embedded NUL rejection. If the accepted PHP surface exposes `$_ENV`, `getenv`,
or related environment APIs, WASM must populate them with PHP-equivalent shapes
or reject the feature through the reviewed capability catalog before emission.

Acceptance covers empty and missing values, duplicate-key handling, non-ASCII
keys and values, embedded NUL rejection, an empty environment, repeated Node
`run()` calls, and identical observations on all three hosts.

### P0 — PHP numeric and error semantics

#### PHP-WASM-NUM-001: shifts

Do not expose WebAssembly's modulo-width shift-count behavior as PHP behavior.
Implement PHP's results for counts greater than or equal to 64 and emit
`ArithmeticError` for negative counts.

Acceptance includes `<<` and `>>` at `-1`, `0`, `1`, `63`, `64`, `65`, very
large counts, negative operands, and `PHP_INT_MIN/MAX`.

#### PHP-WASM-NUM-002: division and remainder

Guard `/`, `%`, and `intdiv()` before the Core numeric instruction:

- zero divisors produce PHP's `DivisionByZeroError`;
- `intdiv(PHP_INT_MIN, -1)` produces `ArithmeticError`;
- `/` preserves PHP float results without losing an integer before conversion;
- neither a Core trap nor `INF` substitutes for a PHP error.

#### PHP-WASM-NUM-003: numeric conversion matrix

Complete and differentially test integer/float/string/Mixed conversions,
comparisons, `NaN`, infinities, signed zero, integer boundaries, precision
boundaries, numeric strings, non-numeric strings, and subnormal values.

The historical audit observed `5e-324` and `1e-320` matching PHP in local
Wasmer and Node runs. That is not a closed gate until a committed differential
regression reproduces it in the pinned host matrix.

### P0 — WASI I/O and termination

#### WASI-IO-001: errno handling

Check the errno result of `args_sizes_get`, `args_get`, `fd_write`, and every
future errno-bearing WASI import. Define one error path that cannot continue
with partially initialized state. `memory.grow` is a Core instruction, not a
WASI errno-bearing import; its `-1` failure result is covered by
`WASM-MEM-001`.

#### WASI-IO-002: complete writes

Loop on `fd_write` until the entire iovec payload is written. Treat a zero-byte
successful write as an error to prevent an infinite loop. Preserve ordering
across mixed string/scalar output.

#### WASI-EXIT-001: non-returning exit

Emit `unreachable` after `proc_exit`. Preserve PHP `exit()`/`die()` integer and
string behavior; frontend restrictions that reject valid PHP arguments must be
fixed in the shared PHP layer and covered on all targets.

The acceptance contract must explicitly define negative and out-of-range PHP
integer statuses at the WASI `proc_exit(i32)` boundary. Host harnesses must
compare the module-level WASI status before any shell-specific 8-bit
normalization, or document and test one common normalization. No successful
local `exit(7)` run alone satisfies this requirement.

The current capability audit rejects `exit`/`die` outside the main function
because caller-owned WASM frames cannot yet be unwound safely. Valid PHP permits
that use, so the rejection is a diagnosed support gap, not compliance. It also
rejects a value-returning main function; the command-mode top-level `return`
value and process-status behavior must be matched to pinned php-src rather than
silently discarded.

### P0 — memory safety and ownership

#### WASM-MEM-001: checked allocator arithmetic

Check alignment rounding, header addition, bump-pointer addition, page
calculation, and i64-to-i32 narrowing before mutation. A failed `memory.grow`
must use a deterministic OOM path, not corrupt state or continue.

#### WASM-MEM-002: unbounded concatenation

Replace the fixed unchecked 64 KiB concatenation area with checked growth or a
heap-backed builder. It must not overlap data, float scratch, heap metadata, or
another frame's live concatenation data.

#### WASM-MEM-003: ownership parity

For strings, arrays, hashes, objects, callables, refcells, `Mixed`, and
iterables, prove balanced ownership across:

- normal return, early return, error, fatal, and process exit;
- overwrite, aliasing, COW, by-reference writeback, and variadic calls;
- closure captures and object destructor re-entry;
- cycles and deliberately malformed runtime cells.

Safety helpers must validate complete block bounds and alignment before reading
headers. Malformed nested `Mixed` chains must terminate with a bounded error or
trap rather than hang.

### P1 — complete EIR and PHP surface

The coverage gate, not this prose, is authoritative. The implementation must
close the following audited families:

| Family | Required work |
|---|---|
| Scalars and strings | string comparisons, strict/loose equality, spaceship, string indexing, persistence, all scalar/string casts, checked mixed numeric operations |
| Arrays and hashes | silent reads, `isset`, mixed keys, append, spread, length, array-to-hash, reference element access, list/unpack behavior, COW |
| Objects and classes | dynamic construction, clone, dynamic properties, initialized checks, static properties, late static binding, reflection paths |
| Calls | named/spread results already normalized by EIR, variadics, by-ref arguments, expression calls, callable descriptors, extern calls, function variants |
| Closures and references | binding, every callable form, refcell binding/loading/release, non-local by-ref storage |
| Exceptions and errors | handler push/pop, catch binding, throw values, fatal terminators, error suppression, cleanup through every exceptional edge |
| Generators and iterators | yield, yield-from, suspension, iterator methods, SPL iterator runtime |
| Globals and statics | writes, `$GLOBALS`, superglobals in the applicable execution mode, static locals and properties |
| Dynamic execution | resolved include/require guards and the supported AOT portion of eval/dynamic dispatch; genuinely impossible AOT behavior requires an explicit public boundary |
| Buffers, pointers, FFI | buffer operations, pointer casts/reads, extern globals/calls, target availability diagnostics |
| Runtime builtins | every variant in the generated current-revision `RuntimeFnId` and unary-string inventories, grouped by their single-source builtin descriptors; the historical 437/15 counts are not fixed requirements |
| Runtime/GC | collection operations, heap metrics, destructors, resource cleanup, callable and object cycles |

Every completed family needs positive PHP-source tests, negative diagnostics,
optimizer-on/off parity where applicable, and ownership tests where values are
refcounted.

### P1 — packaging and host portability

#### WASM-HOST-001: three-host execution matrix

CI must compile real PHP sources once and execute the same validated module on:

- Wasmer;
- Wasmtime;
- Node's WASI API.

The matrix compares stdout, stderr, and module-level exit status. It must pin
the compiler, Wasmer, Wasmtime, Node, external validator, and JavaScript/
TypeScript toolchain versions in its evidence manifest. Tests must have explicit
timeouts and output limits. The job is a required gate; a proposed workflow
patch or a local host run is not evidence until the committed job completes
successfully.

#### WASM-NPM-001: executable package contract

Execute the generated npm package in Node. Test argument, environment, preopen,
exit, thrown-host-error, repeated-run, and concurrent-instance behavior. TypeScript
declarations and JavaScript output must pass their native toolchain checks.

#### WASM-DET-001: deterministic artifacts

Identical inputs, compiler options, and compiler revision must produce identical
WAT, WASM, npm sources, and any package/archive used for distribution after
normalization of explicitly documented metadata.

Acceptance uses separate compiler processes, not two compilations in one
process. The gate records hashes for every output and tests different relevant
map insertion orders. Archive timestamps, entry ordering, permissions, and
compression settings must either be deterministic or be named in the
normalization contract.

## Verification matrix

### Current tested inventory

At the time of this audit, durable repository evidence covers:

- production in-process WAT assembly and WebAssembly Core 3.0 validation with
  `wasmparser`;
- focused artifact-publication tests, including malformed/type-invalid input
  and selected rollback paths;
- [CI run `30439384471`](https://github.com/illegalstudio/elephc/actions/runs/30439384471)
  on Elephc `b3b399408c9`, which validates one artifact with `wasmparser`,
  `wasm-tools` 1.254.0, Wasmer 7.2.1, Wasmtime 47.0.2, and Node 26.3.0, then
  executes it on all three hosts with exact output and `exit(7)`;
- the same job's partial-`fd_write`, repeated Node import, npm file-list,
  reproducible WAT/WASM/package/archive, and strict TypeScript 6.0.3 NodeNext
  checks;
- compile-time exhaustive enum classification, focused shape checks for the
  audited P0 subset, and target-capability rejection tests;
- PHP-source indexed int/bool/string and associative
  int/bool/float/string/container miss regressions across Elephc's 8.2–8.5
  profiles, with exact stdout, ordered key-class-specific warning stderr,
  success status, in-range former-sentinel coverage, precise container hit/miss
  nullability plus indexed/hash/object chained reads, eager single-evaluation of
  later indices on nullable chains, normal offset-on-null warning order, silent
  coalescing side effects, method-on-null status and lazy-argument coverage, and
  import-free silent/reactor coverage;
- focused typed-transfer, `$argc`, void/Mixed result, block-argument, loop, and
  deterministic-ID regressions.

The following observations are useful but are not durable completion evidence:

- local Wasmer 4.3.7 and Node 26.3.0 execution of a hello module;
- local `.lfc` source execution and additional smoke fixtures not retained by
  the pinned job.

No current durable gate proves a complete php-src differential corpus, the full
exit-status/environment/preopen boundaries, exhaustive EIR shapes, or the
adversarial ownership/resource matrix.

### Evidence manifest

Every acceptance run must retain a machine-readable manifest containing:

- the Elephc commit, dirty-state status, command line, PHP fixture list, and
  input hashes;
- the pinned WebAssembly/WASI/php-src revisions and the php-src build
  configuration;
- operating system, architecture, locale, time zone, and all relevant
  environment variables;
- exact compiler, Wasmer, Wasmtime, Node, `wasm-tools`, JavaScript, and
  TypeScript tool versions;
- stdout, stderr, module-level exit status, timeouts, output limits, and hashes
  of WAT, WASM, npm sources, and any normalized distribution archives.

Local observations without that manifest may guide implementation but cannot
change a requirement from partial/open to satisfied.

### Core validation

- in-process production validation;
- external `wasm-tools validate`;
- Wasmer validation;
- Wasmtime compilation;
- Node `WebAssembly.Module` construction.

### PHP differential suites

For each supported `--php-version`, compare against the corresponding maintained
php-src behavior for:

- scalar types, casts, operators, warnings, errors, and error text;
- strings including binary/NUL/Unicode bytes;
- arrays, hash key normalization, order, mutation, COW, and references;
- functions, methods, closures, named/spread/variadic/by-ref calls;
- objects, inheritance, properties, clone, destructors, and magic methods;
- control flow, exceptions, generators, includes, and globals;
- every PHP-visible builtin whose descriptor declares WASM support.

The oracle records stdout, stderr, exit status, and, where observable, type/value
shape. A native Elephc result is useful triangulation but is not the PHP oracle.
Each php-src oracle profile must record whether it uses `php -n` or an explicit
INI, loaded extensions and build flags, error-reporting/display settings,
locale, time zone, architecture, and path/line normalization. The harness must
compare the same logical arguments and environment according to
`WASM-ABI-001`; implicit developer-machine configuration is forbidden.

### Corpus gates

1. Every checked-in example applicable to `wasm32-wasi` must compile, validate,
   and match its oracle.
2. Every checked-in codegen PHP fixture must either pass on WASM or carry a
   reviewed, machine-readable execution-mode exclusion.
3. No emitted example may be invalid.
4. Unsupported counts must trend to zero and final compliance requires zero
   reachable unsupported cases within the declared target surface.

### Adversarial gates

Include bounded tests for allocator overflow/OOM, concat growth, multi-megabyte
partial output, zero-length strings, invalid UTF-8 bytes, malformed heap
pointers, double-free defenses, nested/cyclic Mixed cells, deep recursion,
destructor resurrection, closure capture cycles, COW mutation during borrow,
and repeated host instantiation.

## Documentation requirements

Until all gates pass, public documentation must call the target experimental
and enumerate its tested surface. Claims such as "runs under any WASI host" or
"complete target" are forbidden.

When compliance is reached, update:

- the target matrix and CLI reference;
- npm package and host instructions;
- limitations and execution-mode boundaries;
- roadmap and changelog wording;
- generated coverage reports and test commands.

## Completion rule

The work is complete only when:

1. every area in the status table is **Satisfied** with durable evidence;
2. the exhaustive shape-aware coverage gates report no reachable unsupported
   surface;
3. the full artifact, PHP differential, corpus, ownership, npm, and three-host
   matrices pass;
4. the native first-class target CI passes and a dedicated WASM portability job
   validates and runs one shared artifact on Wasmer, Wasmtime, and Node with the
   external validator and npm gates;
5. three independent available reviewers inspect the exact final source
   revision and the same evidence manifest/log bundle, and each records an
   explicit approval without an unresolved blocker.

An API count, a focused test pass, a valid hello-world module, or agreement
between Elephc's native and WASM backends is progress, not completion.
