# DateTime post-rebase Astra remediation

- [x] R01: idate validation and integer-token semantics in AOT and eval.
- [x] R02: procedural false/warning behavior distinct from throwing OOP methods.
- [ ] R03: procedural aliases bypass overrides while explicit method calls remain virtual.
- [x] R04: shared alias names, arity, named/spread argument binding.
- [x] R05: authoritative request timezone across AOT, eval and callbacks.
- [x] R06: invalid timezone setter emits E_NOTICE, preserving suppression/masks.
- [x] R07: timelib-backed getdate/localtime with wide-year fields.
- [ ] R08: one mktime clock snapshot after supplied argument evaluation.
- [ ] R09: distinguish Mixed box ownership from borrowed/owned raw object returns.
- [ ] R10: concrete date-interface dispatch supports optional/variadic subclass overrides.
- [x] R11 (local discovery): select Magician as the sole TZ archive provider when both are requested.
- [x] R12 (local discovery): string local reuse after unset preserves final Mixed-storage ownership.
- [ ] R13 (local discovery): AOT array_reduce supports omitted null initial carry, not just integer carry.
- [ ] R14 (local discovery): eval debug rendering exposes native DateTime properties, including subclasses.
- [ ] R15 (local discovery): replay PHP octal-overflow warnings from cached eval parsing.
- [ ] R16 (local discovery): capture real Throwable traces and expose them consistently in AOT/eval.
- [ ] R17 (local discovery): support dynamic ReflectionClass construction in AOT, preserving constructor-free defaults.
- [x] M01: getrandmax belongs to Random; regenerate and validate docs.
- [ ] Reconcile every finding with targeted tests and both ABI paths.
- [ ] Update spec/docs and finish required target/PHPT validation.
- [ ] Final squash, push, and clean replacement PR publication.

## Locked audit

### 2026-09-08 checkpoint: timestamp setter target validation

- Extended test_datetime_object_argument_all_target_assembly with native and
  opaque-eval timestamp setters, including reordered named arguments. PASS for
  macOS ARM64, Linux ARM64/x86_64 and iOS device/simulator (47.84s). This verifies
  emission, not executable behavior on every target.
- git diff --check PASS. Remote origin/main remains 640380cf8856379228db77e48c142d7613ed4e48,
  already integrated; remote PR branch remains d07ae8bc7d before this checkpoint.
- Timestamp override/identity, receiver priority, heap, named scalar coercions and
  both audited declaration models have focused passing evidence. Broader coercion
  coverage, other R03 mutations and the campaign's final gates remain open.

### 2026-09-08 continuation: timestamp named-argument regression fixed

- Added test_datetime_procedural_timestamp_set_named_coercions for reordered
  named parameters with numeric strings and booleans in AOT and opaque eval.
  Frozen php-src oracle returns 123|1|456|0|.
- The test initially failed during checking: the resolver appended a positional
  source-line argument after named user arguments. Changed that hidden argument
  to the AST NamedArg sourceLine, following the existing timezone_open pattern.
- The regression now PASSes (20.68s); git diff --check PASS. Full coercion breadth,
  supported-target checks and publication remain open. No task process remains.

### 2026-09-08 continuation: timestamp setter ownership and model validation

- Recovered current worktree state; no earlier timestamp test process remained
  active. Re-ran the focused heap regression rather than assuming its result.
- test_datetime_procedural_timestamp_set_heap PASS (20.88s): repeated AOT and
  opaque-eval calls preserve timestamp 3 and finish with a clean heap summary.
- match_audited PASS (2 tests): both generated declaration variants still match
  their audited test-only models after the receiver guard and return-type change.
- git diff --check PASS. Named/coercion and target breadth validation remain
  before publication of this lot; no full DateTime closure is claimed.

### 2026-09-08 continuation: native procedural timestamp setter

- Registered existing __elephc_date_timestamp_set as a native mutable-receiver
  wrapper, included it in eval reachability and routed eval calls through it with
  an owned, cleaned source-line argument. Its declared result is now DateTime in
  both generated variants and the cfg(test) model, matching receiver identity.
- Added receiver validation before null timestamp deprecation, using the shared
  type-error helper. This preserves PHP's argument-error priority for immutable
  receivers rather than warning about argument two first.
- Native regressions PASS for AOT/eval override bypass and receiver identity, plus
  invalid-receiver/null priority (2 tests, 44.23s). Oracle output verified separately.
- git diff --check PASS. Model, heap, named/coercion and target breadth checks remain
  before the next checkpoint; other procedural wrappers and final gates remain open.
- All task sessions completed. Latest published checkpoint remains d07ae8bc7d.

### 2026-09-08 checkpoint: second origin/main synchronization validated

- Calendar/formatting lot was committed as fcde7f791f and preserved by
  backup/datetime-before-main-640380cf. Rebased all 13 branch commits onto
  640380cf8856379228db77e48c142d7613ed4e48. The sole conflict combined main's nullable
  getenv signature with the get_extension_funcs result shape; neither was discarded.
- Post-rebase exporter build is warning-free; strict calendar mutation test PASSes
  clean (session 39336, 34.35s). Docs were regenerated on the rebased source and
  match the tree: 1022 builtins, 194 classes, 1090 constants, 1939 rendered pages.
  Docs audit zero errors, 1960 site pages valid, EIR boundary zero structural errors.
- Before this rebase, retention no-hook/reference-source tests both passed, as did
  explicit-zone declaration models, pointer alias ownership, big/minimum ISO years
  and five-target assembly emission. These are focused evidence, not full PHPT or
  target execution closure. Other reference alias/container traversal remains open.
- PR #866 stays draft. Final Astra audit, remaining parity fixes and final squash
  are still required. Local-only tool directories remain untracked and backups kept.

### 2026-09-08 checkpoint: calendar formatting and retention preparation

- Stored dynamic properties aliased to Variable/Cell now acquire their own read
  reference before the PCNTL scanner consumes it. The no-debug-hook and reference
  source-survival native regressions both PASS (2 tests, session 30015, 71.94s).
  Other alias targets/private container traversal remain explicitly unclosed.
- Exporter rebuilt warning-free after this change. Regenerated 1022 builtins,
  194 classes and 1090 constants / 1939 rendered pages. Final docs audit has
  917 public and 911 internals pages, zero errors; 1960 pages validate. EIR boundary
  has 632 registry AOT entries and zero structural errors. git diff --check PASS.
- Fresh fetch shows origin/main advanced to 640380cf8856379228db77e48c142d7613ed4e48,
  31 commits beyond the previous base. Commit this validated incremental lot before
  the next rebase, retain backups, then resolve/validate before the next PR push.
- Final Astra, full PHPT/matrix acceptance and the other tracked gaps remain open.

### 2026-09-08 continuation: pointer-read alias ownership closes calendar heap regression

- LLDB confirmed the final 48-byte block contained the formatted calendar result.
  Internal __elephc_ptr_read_string shared the public helper's copying implementation
  but not its RuntimeFnId result-ownership classification. Added ElephcPtrReadString
  to the same owned-result bucket as PtrReadString, with an alias-contract unit test.
- Strict calendar mutation regression now PASSes, including absence of Notice
  output and clean heap (session 52535, 20.86s). Alias ownership unit PASSes;
  big-year PHPT regression and minimum ISO-year regression both PASS.
- Five-target assembly validation PASSes (session 20981, 48.54s). All task sessions
  completed. Docs refresh and broader formatter/scanner alias checks remain before
  next publication. No final audit or full parity closure.

### 2026-09-08 continuation: separate retention traversal from debug projection

- Added eval_object_storage_properties and routed PCNTL's post-eval scan through it.
  It bypasses user __debugInfo and native DateTime virtual snapshots. Native date
  subclasses still expose stored user properties; dynamic traversal excludes virtual
  properties. Explicit debug rendering keeps its original projection path.
- Native no-debug-hook regression PASSes (session 18131, 31.88s). Calendar mutation
  regression emits no unexpected notices now, but still FAILS strict heap with one
  block / 48 bytes (session 6064, 20.45s). Do not weaken the clean assertion.
- This is not a complete physical-GC walker: dynamic reference-target reads retain
  legacy semantics and ownership, generic native private/container references need
  further audit, and closure capture traversal must preserve foreign-context detection.
  These remain explicit follow-ups before claiming the retention walk is fully inert.
- git diff --check PASS before the final validation. No task process remains active;
  all changes after checkpoint 5fcbbbb216 remain local pending the outstanding gates.

### 2026-09-08 continuation: explicit-zone AST model validation

- Ran the two generated-declaration versus audited-model tests. The new test-only
  timelib format source first lacked its PHP opening tag, then duplicated the
  initialization guard already injected by the model builder. Corrected both
  reference-model issues without removing the production initialization guard.
- Both match_audited tests now PASS (session 78040, 4.02s): timezone extern
  declarations and both DateTime declaration variants remain structurally aligned
  with their cfg(test) models. No production PHP parsing was introduced.
- The post-eval PCNTL storage walk remains the active blocker for the strict
  calendar test; do not route it through debug projections or hooks. Native raw
  property reads own their boxed results, whereas reference-target readers mix
  borrowed and owned values and can execute access hooks. Builtin compact/container
  layouts also preclude blindly treating reflection metadata as raw GC layout.
- No task process remains active. Latest published checkpoint remains 5fcbbbb216;
  native/target/formatter breadth and scanner corrections are still required.

### 2026-09-08 continuation: PCNTL retention scan invokes debug hooks

- Local timelib DateTime/Immutable format bodies now use elephc_tz_format with an
  explicit timezone, localtime flag and null output-length pointer; existing civil
  formatting remains. Added AST extern declaration and updated cfg(test) reference
  models. getOffset uses the native format wrapper's Z token. Model/target/full
  formatting validation still pending; these changes are not published yet.
- Calendar heap regression still has one notice and one 48-byte allocation. LLDB
  proves the unexpected caller is NOT the explicit format/getOffset operation:
  execute_parsed_eval -> value_contains_foreign_pcntl_callable ->
  eval_debug_object_properties -> eval_native_date_debug_properties ->
  DateTime::__elephc_debug_properties (cloned serialization body).
- Added test_datetime_eval_retention_scan_does_not_call_debug_info. Native run
  FAILS: body|unexpected-debug|after instead of body|after. Session 32125 completed.
  The retention scanner must inspect stored references, not execute user __debugInfo
  or DateTime's virtual debug projection. Do not suppress notices or skip arbitrary
  objects merely to green the test.
- Raw property_get bridge inspected: native slots box values directly; stdClass
  reads retain the stored Mixed cell. Neither needs debug projection. Dynamic
  reference targets need care: eval_reference_target_value has mixed ownership and
  ObjectProperty resolution can invoke PHP access hooks, so do not reuse it blindly
  for a side-effect-free owning graph walk. Include private/backed/reference fields
  and retain PCNTL foreign-closure detection while making the walk non-observable.
- Public DateTime serialization/debug still needs explicit-zone parity separately;
  fixing the scanner alone is not proof those surfaces emit correct fixed-offset data.
- No task process remains active. Other worktree builds observed were unrelated and
  left untouched. Existing draft PR checkpoint remains 5fcbbbb216.

### 2026-09-08 continuation: calendar mutation heap and fixed-offset notices

- Added strict calendar setter heap regression (four eval rounds, including a
  borrowed DateTimeZone argument). Initial result had 6 blocks / 600 bytes plus
  unexpected notices for internal timezone ID UTC-2.
- Extended fetched argument-box cleanup to Object parameters, under the existing
  independent-return-box guard and argument-array lifetime guarantee. Strict test
  improves to 1 block / 48 bytes (session 60657, 28.73s); notices remain unchanged.
  The regression now explicitly rejects Notice output as well as requiring clean heap.
- Native DateTime::setTimezone already uses private DateTimeZone::__elephc_export_name,
  not virtual getName; do not invent an argument-getName override gap here.
- Confirmed timelib generated DateTime::format/getOffset still temporarily call
  date_default_timezone_set(__elephc_runtime_timezone_name(timezone_name)); this
  exposes POSIX UTC-2 to public PHP validation. Both mutable/immutable bodies share
  the issue. Investigate explicit-zone bridge operations rather than suppress notices.
- Last block still requires direct inspection. No task build active; new wrappers
  and Object-argument cleanup remain local after published 5fcbbbb216. Final gates open.

### 2026-09-08 continuation: procedural calendar and timezone mutation wrappers

- Extended the existing AST/native mutable-receiver wrapper inventory to date_date_set,
  date_isodate_set and date_timezone_set. AOT rewriting, eval aliases and eval-reachable
  methods now use those wrappers rather than virtual calls on the receiver.
- Native combined regression PASSes for AOT and opaque eval (53.86s): leap-day setting,
  ISO week default day, timezone adjustment, returned receiver identity and explicit
  virtual override calls. Exact output also verified with the frozen php-src oracle.
- git diff --check PASS. These changes are local after published checkpoint 5fcbbbb216;
  memory/argument-edge/target checks and docs refresh remain before the next push.
  Other R03 mutations and all remaining campaign gates stay open. No task build active.

### 2026-09-08 checkpoint: rebased PR branch ready for incremental publication

- Local branch now feat/datetime-php-src-compliance-v2, tracking fork/v2 for PR #866.
  Rebase base is origin/main a5c29be9605d6bb144ab97ab8357a74e4ff4cb4e; all source and
  restored-stash conflicts resolved. Original/stash/published/rebased backups retained.
- cargo check --tests completed without errors; its one unused-import warning was
  removed. Subsequent exporter build completed warning-free on the merged source.
- Post-rebase validation PASS: 32 lexer tests, 25 context tests, 2 PCNTL tests,
  60 native-dispatch tests, 25 builtin-contract tests; these are overlapping filters,
  not a summed unique-test count. Native time-set group 3 PASS (including strict
  clean heap), mixed string getter/property preservation 1 PASS, five-target
  assembly fixture 1 PASS (all five emissions; not cross-target executable proof).
- Generated docs refreshed: 1022 builtins, 194 classes, 1090 constants; 1939 pages
  rendered. Final docs audit 917 public / 911 internals, zero errors; 1960 site pages
  validate. EIR boundary 632 AOT registry entries, zero structural errors. Compatibility
  page regenerated separately. Final Astra, exhaustive PHPT and remaining gaps open.
- This is an incremental draft checkpoint, not a compliance closure. Known pending
  reflection/trace and omitted-array_reduce-initial regressions remain enabled.
  Local-only graft/ and .agent_memory/ must not be staged or published.

### 2026-09-08 continuation: origin/main rebase and dirty-state restoration

- Preserved original HEAD in backup/datetime-before-pr866-sync-20260908 and all
  dirty tracked/untracked state in stash 6255f56c64b672d011707a087353c26bcc808820,
  also pinned by backup/datetime-dirty-state-20260908. Published PR commit is backed
  up separately. Reflog proves fork/v2's 8941224062 is the historical branch head.
- Rebased all 11 local commits onto origin/main a5c29be9605d6bb144ab97ab8357a74e4ff4cb4e;
  resulting HEAD 94fdbbb5ed. Preserved in backup/datetime-rebased-before-local-restore-20260908.
- Applied the full dirty stash; all new Rust files and local-only tool directories
  restored. Resolved 988 generated-doc/registry conflicts provisionally with the
  rebased copies; regenerate these from the merged source before publication.
- Source merges retain main's PCNTL/fatal/Closure changes alongside datetime
  diagnostics, owned eval results, callback cleanup and spread-overflow contracts.
  PCNTL context deferral now reserves an ABI owner until deferred release; final
  cleanup retains panic containment and honors other retained context owners.
- Retained main's structured class-constant default enum and adapted the branch
  constructor. Combined main's hex escapes with byte-preserving octal warnings;
  hex escapes now also append bytes directly. Keep associated lexer tests green.
- All conflict markers are resolved (timelib README '=' underline is not a conflict).
  Cargo metadata and git diff --check passed. ACTIVE cargo check -p elephc --tests
  session 9563; poll before further builds. No push yet. Critical follow-up gates:
  retained/deferred PCNTL context tests, native/callable ownership, lexer, time-set
  heap, mixed string property preservation, target emission and full docs workflow.
- Operational checkpoint also lives at /tmp/elephc-datetime-binding.yhuLdd/pr866-sync.md.
  Keep the stash and backup refs until validated publication to fork/v2.

### 2026-09-08 continuation: clean time-set heap and live PR synchronization requirement

- LLDB proved the last 48-byte block was the final formatted string. Main EIR
  releases the boxed method result correctly; store_mixed_method_call_result used
  the borrowing boxer for Str because PhpType::is_refcounted excludes string pairs.
  Include Str in the owned-result transfer path, consistent with eval method boxing.
- Strict test_datetime_procedural_time_set_heap PASSes clean (session 89423, 21.10s).
  Added test_datetime_mixed_string_result_preserves_property as a counter-regression
  for getter property ownership; NOT RUN yet. ASM alignment and diff checks PASS.
- User explicitly requested regular pushes to the OPEN PR and rebases on origin/main.
  Live GitHub verification supersedes older notes saying no replacement PR exists:
  PR #866 is draft, head Guikingone:feat/datetime-php-src-compliance-v2 at
  8941224062d1de2e76b114ed7526ea674250cff1 (Sep 1 squash), parent 42a77e8f7e.
  The worktree is still on local feat/datetime-php-src-compliance at c423b1d56d.
- Fetched origin and explicitly fetched fork/v2. origin/main is now
  a5c29be9605d6bb144ab97ab8357a74e4ff4cb4e; HEAD is 11 ahead / 78 behind it.
  HEAD versus fork/v2 is 217 ahead / 1 behind, merge base 42a77e8f7e.
  Dirty worktree has 1972 entries including 20 untracked entries; many generated
  docs and untracked Rust files, plus local-only graft/.plans/.agent_memory state.
- BEFORE more implementation: establish a reversible dirty-state preservation
  boundary, reconcile the existing fork PR commit with local campaign changes,
  rebase onto current origin/main, resolve conflicts, validate and publish regular
  checkpoints to fork's v2 branch. Do not overwrite its unique commit blindly or
  push to the obsolete origin branch. No rebase or push has happened in this turn.
- No task tool process remains active. Final Astra/PHPT/matrix closure is still open.

### 2026-09-08 continuation: fetched Mixed argument box ownership

- Corrected the initial result-ownership hypothesis: expression statements already
  release their eval result. The leaked receiver cell comes from fetched Mixed
  native arguments, excluded by release_staged_scalar_box's parameter-type gate.
- Bridge argument_array owns its stored input cells throughout invocation. The
  staged fetched reference can be dropped for Mixed parameters when the return is
  independently boxed; potential Mixed-return aliases remain excluded.
- Extended the shared gate accordingly and added both-target assertions for Mixed
  input/Object output release versus Mixed input/Mixed output preservation.
- Native strict time-set heap regression drops from 6 blocks / 696 bytes to
  1 block / 48 bytes (session 80450, 20.86s). It remains RED; locate the final block
  without weakening the assertion. Previous inspection showed a formatted string.
- Assembly comment alignment and git diff --check PASS. Session 67304 completed:
  all three eval_arg_ownership unit tests PASS. Direct callers are method/constructor
  bridges (not generic callable-array function invokers); their Rust adapters pack
  arguments through the owned Mixed argument_array. No task build remains active.
  No final audit or publication.

### 2026-09-08 continuation: native default-owner ledger implementation

- Native binder now records newly materialized defaults and its new variadic array
  in an explicit caller-owned ledger. Instance, static and constructor dispatch all
  carry it across binding/invocation/writeback and clean it on success or failure.
- Shared finish_native_default_owners preserves returned-cell aliases and pending
  Throwable owners. Instance EvalExprResult marks a transferred default owner owned.
  Coercion-created replacements are not assumed covered by this default ledger.
- Native unit filter PASSes 58 tests; two added cleanup regressions PASS (failure
  releases and returned/pending owner preservation). git diff --check PASS.
- Strict time-set heap regression now has 6 blocks / 696 bytes, down from
  14 / 1080: eight omitted-default cells recovered (session 72744, 34.01s).
  Test remains RED; remaining receiver-result ownership and other residual cells
  still require evidence and correction. All task sessions completed.
- No final audit, squash or push; remaining campaign requirements unchanged.

### 2026-09-08 continuation: native omitted-default ownership root cause

- Located omitted default creation in statements/native_argument_binding.rs,
  bind_native_signature_args: materialize_native_callable_default fills missing
  BoundMethodArg values without an owner ledger. BoundMethodArg only carries value,
  ref_target and variadic_ref_targets. The variadic array is similarly created here.
- Native static execution in native_method_execution.rs binds, invokes, writes
  references back and validates the return, but does not release generated default
  owners. This explains the next eight zero-valued cells in four time-set calls.
- Required fix: track actual default owners from materialization, including partial
  binding failures; release after invocation/writeback while preserving returned
  aliases and pending throwables. Do not infer ownership solely from handle inequality
  or release every bound argument: caller variables are borrowed and coercion can
  replace values. Instance/constructor consumers must be audited with the same binder.
- No new production edit or test run in this diagnostic checkpoint. Strict time-set
  heap test remains RED at 14 blocks / 1080 bytes. No task process active.

### 2026-09-08 continuation: isolate time setter eval ownership and source scalars

- Dedicated CLI fixtures time_set_aot.php/time_set_eval.php under the campaign
  temp directory show AOT-only mutation is clean; eval-only retains 18 blocks.
  LLDB sees receiver Mixed box with refcount four, its DateTime/hash/timezone owners,
  twelve zero-valued scalar cells, an uninitialized cell and a formatted string.
- eval_date_procedural_alias_call previously used eval_call_arg_values without a
  source-owner ledger. It now uses eval_call_arg_values_with_temporaries and releases
  proven scalar owners after the alias finishes, preserving returned-cell aliases
  and the original evaluation failure. No container ownership was guessed.
- Strict combined heap regression improves from 18 blocks / 1272 bytes to
  14 blocks / 1080 bytes (session 11749, 34.16s), matching four literal minute
  argument cells recovered. Test remains RED. Next inspect native default argument
  owners (two omitted defaults per call) and alias result ownership/refcount.
- git diff --check PASS. All task compiler/debugger sessions completed. These
  temporary fixtures remain available; no final audit, squash or push performed.

### 2026-09-08 continuation: time setter argument and ownership validation

- Added named/default-argument and DateTimeImmutable rejection regression for
  date_time_set in both AOT/eval. Frozen php-src oracle matches exact output,
  including microsecond reset when omitted and public receiver TypeError text.
- Both procedural time setter behavior tests PASS (session 38914, 42.20s).
- Added strict heap regression for four discarded AOT mutation returns plus four
  eval mutation returns, followed by receiver/source cleanup. Correct final time
  prints, but heap FAILS with 18 live blocks / 1272 bytes (session 98953, 21.20s).
  Keep this test red and isolate AOT versus eval ownership before expanding the
  mutator list. Behavioral alias correctness is not clean-heap proof.
- git diff --check PASS. All task test sessions completed. Other R03 mutations,
  docs refresh, targets and final campaign gates remain open.

### 2026-09-08 continuation: native date_time_set mutation wrapper

- Added NATIVE_PROCEDURAL_MUTATORS and owner lookup, separate from read wrappers.
  AST wrapper builder installs __elephc_date_time_set; resolver and Magician aliases
  route procedural calls there; eval-reachable method inventory includes it.
- Date-interface lowering recognizes mutators and dispatches exactly to DateTime,
  not DateTimeImmutable or subclass overrides. Receiver errors use DateTime rather
  than DateTimeInterface. Static return alias explicitly names parameter zero.
- Native regression PASSes (33.08s): AOT/eval mutate the same subclass instance,
  return receiver identity, bypass setTime override, and preserve explicit virtual
  method calls. Frozen oracle expected output was confirmed in the prior checkpoint.
- git diff --check PASS. Named/default argument combinations, immutable receiver
  rejection, strict heap and targets remain to validate; other R03 mutators remain.
  Builtin docs workflow must refresh after this alias change before publication.
- Session 93301 completed successfully; no task build active. No final audit/push.

### 2026-09-08 continuation: procedural time mutation regression

- Resumed R03's remaining DateTime mutators without dropping R16/R17. Added
  test_datetime_procedural_time_set_bypasses_override, checking AOT and opaque eval,
  mutation, returned receiver identity, and explicit virtual setTime calls.
- Frozen php-src oracle confirms exact output:
  12:34:00|same|override|05:06:07|same|override|
- Native regression FAILS: both procedural calls invoke the override and leave
  the timestamp unchanged. name_resolver/expressions.rs rewrites date_time_set
  directly to setTime; Magician time/aliases.rs likewise uses eval_method_alias_tail.
- Existing native_procedural.rs builder installs only read wrappers. Next mutator
  fix must preserve receiver-return aliasing (not reuse read-only ReturnArgAlias::None),
  reject DateTimeImmutable receivers as PHP does, and keep explicit method calls
  virtual. lower_date_interface_call currently allows both date families for reads.
- Session 87356 completed RED. No build remains active; no production change or
  final audit/push in this checkpoint. The new regression remains enabled.

### 2026-09-08 continuation: previous-owner acceptance and real trace gap

- Native test_datetime_eval_parse_error_previous_owner PASSes (20.70s): the object
  returned by getPrevious survives unsetting its containing exception inside eval.
- Confirmed AOT lower_throwable_standard_method_loaded unconditionally routes
  getTrace to lower_throwable_empty_trace_array and getTraceAsString to an empty
  string; __toString also only returns the message. These are not PHP trace parity.
  Do not simply copy an empty-trace result into eval to green the reflection test.
- Runtime search found specialized uncaught date/unserialize trace state and
  formatting in exceptions/throw_current.rs, not a general per-Throwable captured
  trace. R16/R17 now have explicit checklist entries rather than hiding these
  discoveries in R15 warning replay. The full reflection test remains RED.
- All task processes completed. No new production edits or final audit/push in
  this checkpoint; getPrevious non-null lifetime now has direct native evidence.

### 2026-09-08 continuation: eval Throwable getPrevious dispatch

- Identified the getter failure: eval_method_helpers.rs special builtin Throwable
  dispatch only exposed getMessage/getCode, not getPrevious. Added getPrevious
  name dispatch and nullable payload boxing on ARM64/x86_64. Null uses tag 8;
  a non-null previous uses tag 6 and the shared boxer retains the object owner.
- Added test_datetime_eval_parse_error_previous_owner: fetch previous inside eval,
  unset the containing exception and read the retained previous message. Not run yet.
- Session 98080 completed RED: output advanced from `[]|0|` to `[]|0|null`, proving
  the null getPrevious branch now executes. Failure moves to getTrace/count; keep
  the complete regression red until that path is corrected. No build remains active.
- Assembly alignment and git diff --check PASS before the latest test-only edit.
  getTrace and other unsupported bridge methods have not been certified, and the
  dynamic ReflectionClass AOT regression remains open. No final audit/push.

### 2026-09-08 continuation: refined initializer validation and reflection gaps

- Session 60668 completed: parse_error filter 4 PASS / 1 FAIL. Current refined
  empty-thunk implementation passes strict clean-heap and catch regressions.
- New constructor-free reflection test is RED before execution: AOT
  ReflectionClass::__construct rejects a dynamic class name (requires a literal).
  Preserve this test; dynamic reflection is not validated by the catch/heap passes.
- Added separate eval reflection fixture without removing the AOT regression.
  It executes and prints `[]|0|`, then exits with eval runtime failed while evaluating
  the getPrevious/null comparison (before trace count). Message and code defaults
  are observed correct; nullable previous/remaining getter dispatch needs diagnosis.
  Do not assume this is caused by the initializer change without comparative proof.
- Five-target eval/native-bridge assembly fixture PASSes (47.32s), emission only.
  Session 56580 completed; eval reflection session 37332 completed RED; all task
  sessions are terminal. git diff --check passed before the latest test-only edit.
- No full reflection/target executable closure or final campaign completion claim.

### 2026-09-08 continuation: compact Throwable property initialization fix

- Moved the existing compact Throwable class predicate to types/builtin_classes.rs;
  codegen delegates to it and EIR property initialization now uses the same layout
  decision, without an ir_lower dependency on codegen.
- First candidate skipped generic property thunks for compact exceptions; the
  strict native heap regression PASSed clean (session 48934, 20.51s), eliminating
  the eight remaining initializer allocations.
- Refined the candidate to KEEP an empty property thunk rather than remove it:
  runtime_class_infos clears declared defaults when no thunk exists. The empty
  thunk preserves that metadata, while by-name allocation's existing zeroing
  supplies empty message/code/previous without heap-backed generic defaults.
- Added test_datetime_parse_error_without_constructor_defaults using dynamic
  ReflectionClass/newInstanceWithoutConstructor and message/code/previous/trace.
- ACTIVE session 60668: cargo codegen_tests filter parse_error on the refined
  current source. Poll before further builds; first clean result predates the
  metadata refinement and is not final validation of the current revision.
  git diff --check PASS; target/reflection coverage and final campaign gates open.

### 2026-09-08 continuation: remaining ParseError property initializer allocations

- Recompiled the CLI reproducer after scalar bridge cleanup; LLDB confirms the
  remaining eight blocks are four uninitialized Mixed cells and four raw buffers.
  Allocation backtraces show exactly four raw 32-byte requests and four boxed-cell
  requests from class_propinit_55, reached by _rt_new_by_name for ParseError.
  Allocator reuse explains larger live block capacities; total remains 448 bytes.
- Diagnostic script /tmp/elephc-datetime-binding.yhuLdd/trace_parse_alloc.py records
  allocation origins; the reproducible binary/source remain parse_error_heap(.php).
- The general by-name allocator initializes ordinary property defaults before the
  eval constructor uses compact Throwable fields. Compact construction overrides
  these fields without consuming generic default owners. Next fix must reconcile
  compact Throwable allocation/property initialization on both targets, preserving
  constructor-free allocation semantics; do not globally remove property init.
- Relevant paths: ir_lower/function.rs lower_property_init_thunk; ir_lower/program/
  declaration_metadata.rs lower_property_init_thunks; codegen/runtime_metadata/
  classes.rs runtime_class_infos; runtime/objects/new_by_name.rs; compact Throwable
  body/default initialization in codegen/eval_constructor_helpers.rs.
- No production edit this checkpoint; strict heap regression remains RED. All
  debugger/compiler sessions completed. Final campaign gates remain open.

### 2026-09-08 continuation: Throwable bridge scalar-cell cleanup

- ARM64 and x86_64 builtin Throwable constructor bodies now release fetched
  message/code cells after storing their converted values. Existing shared
  release_staged_scalar_box handles both cached-cell layouts.
- Confirmed mixed_cast_string's string arm persists an independent raw string;
  releasing the fetched source box after storing that copy does not invalidate
  the exception message. Integer values are already copied into the payload.
- Native strict heap regression now reports 8 blocks / 448 bytes instead of
  20 blocks / 1064 bytes, proving 12 allocations are recovered across four throws.
  The clean-heap assertion remains RED; remaining placeholders/raw allocations
  must still be identified and corrected. Session 95255 completed, no build active.
- Eleven pre-existing multiline instruction comments in the touched helper were
  relocated to instruction opening lines, with all instruction text preserved.
  Assembly-comment alignment and git diff --check PASS.

### 2026-09-08 continuation: strict ParseError heap regression and allocation evidence

- Added test_datetime_nested_parse_error_heap: four nested failures, catch/unset
  each exception, then unset both source variables; requires a clean heap.
- First run failed linking with ENOSPC. Compressed ten old main.s artifacts from
  elephc_eval_string_return_targets_{82877,10056}_ThreadId(2)_{0..4}; all gzip
  integrity checks passed. No sources deleted. Interrupted session 2677 disappeared;
  no Cargo/rustc/test processes remained before restarting. Disk later had 11 GiB free.
- Fresh run session 79676 executed and FAILS with 20 live blocks / 1064 bytes.
  CLI fixture /tmp/elephc-datetime-binding.yhuLdd/parse_error_heap.php reproduces
  exactly; LLDB heap inspection confirms four message strings, four message boxes,
  four integer boxes, four null/uninitialized boxes, and four raw kind-1 allocations.
  The linked Magician archive is newer than the helper cleanup, not a stale archive.
- Concrete remaining bridge gap: eval_constructor_helpers.rs builtin Throwable
  bodies fetch fresh argument boxes into the cached-cell slot, cast/store message
  and integer code, then fetch the next argument without releasing the previous box.
  Both ARM64 and x86_64 paths do this. eval_arg_ownership::release_staged_scalar_box
  already knows both layouts. Audit raw message ownership before adding its release;
  distinguish remaining default/placeholder allocations from the fetched boxes.
- No task process remains active. Strict heap test remains RED. This is allocation
  evidence, not a claim that the entire 20-block leak has been explained or fixed.

### 2026-09-08 continuation: ParseError constructor temporary cleanup

- Runtime construct_object packs borrowed source cells into argument_array and
  releases that array, not the caller's source cells. eval_throw_parse_failure now
  tracks its message/code owners and releases both after construction, including
  partial-allocation/construction failures. A failed construction releases the
  exception owner; success transfers that owner to pending_throw.
- Focused unit test PASSes: exactly two argument releases, no exception release.
  Native nested ParseError regression PASSes after adding a getMessage read inside
  catch (35.97s). git diff --check PASS. All task sessions completed.
- This is not universal clean-heap proof: throwing/catching and legacy eval source
  temporary lifetimes still require a strict heap regression. Exact PHP diagnostic
  metadata and the other R15/campaign requirements remain open.

### 2026-09-08 continuation: nested native acceptance and generated class docs

- Native inner ParseError catch regression completed PASS (33.26s), confirming
  execution resumes after catch inside opaque eval, not only at the outer AOT frame.
- update-builtin-docs completed after the new class contracts: exporter builds
  warning-free with curl; registry has 987 builtins, 192 classes and 1089 constants;
  1869 pages rendered. Docs audit zero errors; 1890 generated pages validate.
  EIR structural audit passed in the immediately preceding checkpoint, with no
  production source edits since. git diff --check PASS.
- All task tool sessions completed. Remaining ParseError work includes precise
  message/file/line metadata, owned temporary cleanup proof, inheritance and target
  coverage, and native include coverage. R15/error_get_last and other campaign
  findings remain open; no final audit, squash, push or replacement PR yet.

### 2026-09-08 continuation: nested eval/include ParseError propagation

- Added eval_throw_parse_failure: real ParseError statuses allocate a ParseError
  through RuntimeValueOps, schedule pending_throw and return UncaughtThrowable.
  UnsupportedConstruct remains distinct. Both nested eval and include use it after
  replaying warnings; include still restores the caller context.
- Existing two failed-parse warning regressions PASS with Throwable propagation.
  Added native test_datetime_nested_eval_parse_error_is_catchable to require an
  inner catch and execution of the following statement. Generic message and
  exception file/line remain incomplete, as does explicit new-helper ownership proof.
- Native test session 26455 completed successfully: inner catch and subsequent
  statement both execute (1 PASS, 33.26s). No native test remains active.
  EIR boundary audit session 23700 completed: 597 registry AOT builtins, zero
  structural errors. git diff --check PASS.
  Generated docs/exporter still need refresh for the prior class catalog additions.

### 2026-09-08 continuation: ParseError class and AOT unwinding implementation

- Added shared CompileError and ParseError class contracts and checker declarations
  with ParseError -> CompileError -> Error inheritance. Included both in signature
  patching, unconditional throwable registration, native constructor/method bridges,
  dynamic construction candidates and compact Throwable payload recognition.
- Seeded runtime class metadata and _spl_parse_error_class_id. Eval status 1 now
  calls shared target-aware emit_static_exception through emit_parse_error instead
  of exiting through the fatal writer. Diagnostic message is still generic; do not
  claim PHP message/file/line parity or catchable nested eval/include yet.
- Assembly comment checks and git diff --check PASS. Fixed three pre-existing
  multiline comment placements in the touched exception emitter, instructions unchanged.
- Native test_datetime_eval_octal_compile_warning_failed_parse PASSes (1 test,
  22.78s execution): both cached failures warn and are caught as ParseError. Session
  70590 completed successfully; no task build remains active. Disk last observed
  3.7 GiB free. Other targets and broader exception tests remain unverified.
- update-builtin-docs skill read: exporter/regeneration/audits MUST be rerun after
  this class catalog change; not yet run. No final Astra/squash/push performed.

### 2026-09-08 continuation: catchable ParseError root cause

- Confirmed two independent obstacles behind the native failed-parse regression:
  shared catalog_classes.rs declares neither ParseError nor CompileError; and
  codegen/lower_inst/builtins/eval/status.rs explicitly maps status 1 to
  emit_eval_fatal_message, which writes stderr and exits instead of unwinding.
- Existing exception integration points are checker/builtin_types/declarations.rs
  inject_builtin_throwables, exception.rs patch_builtin_exception_signatures,
  builtin_class_gate.rs ALWAYS_REGISTERED_THROWABLES plus its seed-list test, and
  the shared class catalog. The PHP inheritance chain must be preserved when
  adding ParseError (including CompileError), not merely accepted in catch syntax.
- Next implementation must connect parser failures to a real owned Throwable and
  the existing normal unwinder across both ABIs; audit nested eval/include as well
  as the FFI entry. Preserve diagnostic warnings before exception construction.
- This checkpoint is read-only code diagnosis; no additional validation or
  generated-doc regeneration was claimed. Existing native regression remains red.

### 2026-09-08 continuation: native parse-error regression and nested lines

- Corrected a formatting-command mistake from the previous checkpoint that removed
  the scanner closure delimiters after its test run. Current lexer suite PASSes
  31 tests, including failed interpolation warning source-line offsets.
- Nested-token errors now apply the same outer-source line adjustment as successful
  token batches. Incomplete captured expressions still need coverage.
- Native warning tests: successful cached replay/mask test PASS; new
  test_datetime_eval_octal_compile_warning_failed_parse FAILS at checking:
  `Undefined class: ParseError` for both catch clauses. Keep this regression red;
  catchable PHP parse errors must be implemented, not replaced with a weaker test.
  Graft found no ParseError declaration in the shared builtin contract.
- error_get_last also has no literal implementation hits in src/ or Magician
  builtins; investigate its catalog/runtime support rather than assuming existing
  diagnostic bookkeeping. No final audit, squash or push performed.

### 2026-09-08 continuation: unfinished string diagnostics

- Frozen php-src oracle confirms overflowing octal escapes warn even when the
  double-quoted string has no closing quote, before the caught ParseError.
- Double-quoted scanning now attaches collected warnings to every failing scan
  result, including trailing backslash and incomplete interpolation paths.
- Lexer filter PASSes all 30 tests. Cached-failure coverage now includes missing
  closing quote, trailing backslash and incomplete braced interpolation.
- R15 remains OPEN: nested interpolation failure line offsets and warnings inside
  incomplete captured expressions require scrutiny; error_get_last bookkeeping
  and native diagnostic regression validation remain incomplete.

### 2026-09-08 continuation: replay failed-parse diagnostics

- Shared warning emission now accepts typed warning slices from either a program
  or a parse error. All three cached-parser consumers replay error warnings before
  returning their original status: production FFI, nested eval, and include.
- Include installs its file context before replay and restores the caller context
  on both parse failures and diagnostic failures. Successful programs retain their
  single existing emission path.
- Two focused interpreter regressions PASS: repeated cached nested-eval failure,
  and included-file failure with exact file label and caller-context restoration.
- Production cargo check initially found an unused test-only EvalParseError import;
  it is now cfg(test). No native executable test was run in this checkpoint.
- R15 remains OPEN for warnings within unfinished string tokens, error_get_last
  bookkeeping, full nested-location parity, and native FFI regression coverage.

### 2026-09-08 continuation: failed parse warning metadata

- EvalParseError now retains typed compile warnings while preserving its original
  ABI status. Parser grammar failures and lexical failures after a completed token
  batch attach preceding warnings; cached failures clone that metadata unchanged.
- Focused `cargo test -p elephc-magician --lib octal_compile_warnings` PASSes:
  2 tests, including grammar failure and unterminated comment after an overflowing
  octal string. `git diff --check` PASSes.
- R15 remains OPEN: warnings inside an unfinished double-quoted token are still
  lost. The three cached-parser consumers (ffi/execute.rs and nested eval/include
  in interpreter/include_exec.rs) still map failures straight to status without
  replaying their newly retained warnings. Wire emission without duplicating the
  successful-program path, then cover error_get_last and nested source locations.
- No native end-to-end validation, final Astra review, squash or push performed
  in this checkpoint.

### 2026-09-08 continuation: cached eval compile-warning replay

- Frozen PHP oracle established octal overflow is E_COMPILE_WARNING (128), not
  E_WARNING, and is not sent to the user error handler. Values still wrap to bytes.
- Lexer retains typed EvalCompileWarning metadata with source lines. Parser removes
  diagnostic tokens from grammar input and attaches warnings to EvalProgram. Nested
  interpolation warnings preserve lexical ordering; cached Arc programs retain them.
- Program execution replays warnings before its first statement on every invocation.
  Include execution uses its own source label and restores caller context after
  diagnostic errors. RuntimeValueOps has a distinct compile_warning operation;
  ARM64/x86 wrappers gate bit 128 and use the raw diagnostic writer, preserving @
  without applying the runtime E_WARNING mask or user-handler dispatch.
- Native replay/mask regression PASSes: two executions of identical cached source
  warn under mask 128; mask 2 alone and @ suppress output; all four bodies execute.
  Unit filters PASS: lexer 29, parser 216, core 40. Five-target emission PASSes.
- R15 remains OPEN: parse failures currently discard their collected warnings
  because parse_fragment attaches metadata only to successful EvalProgram results.
  Lexer failures after an overflow and error_get_last diagnostic bookkeeping also
  need verification/implementation. Do not claim full diagnostic parity from the
  successful-program replay test. Source locations for nested eval/include need
  full parity coverage as part of that remaining audit.
- No task process remains active. Final Astra/squash/push and the other tracked
  R03/R09/R13/R14 validation/implementation requirements remain incomplete.

### 2026-09-08 continuation: debug hooks, octal keys and reference-return bridge

- Added core/debug_hooks.rs. Declared __debugInfo methods run before default native
  projections, with the correct declaring-class scope; lookup does not invoke __call.
  Hook arrays preserve numeric keys and visibility-mangled string keys. Null results
  produce an empty property table and the PHP 8.5 deprecation branch is implemented.
  Unknown hook result owners remain conservative; no blanket release was added.
- Native non-null array return ABIs create fresh Mixed boxes, now reflected in eval
  result ownership. A second leak remained in their raw array/hash payloads. EIR of
  DebugHeapDate::__debugInfo proved its hash return Owned. A callee proof now adopts
  arrays only when every return is Owned and the method is not by-reference; all four
  instance/static ARM64/x86 bridge paths preserve actual indexed/hash tags.
  test_datetime_eval_debug_hook_heap PASSes clean (previously 8 blocks / 4512 bytes).
- The by-reference hook regression exposed a real SIGSEGV: native methods returned
  reference-cell pointers but eval treated them as array payloads. New
  src/codegen/eval_method_results.rs centralizes result boxing and dereferences borrowed
  return cells. Method slot metadata carries by_ref_return. Native property getters
  likewise recognize reference slots. The original reference cell is not consumed.
  Reference writes/rebinding and other unproven R09 paths still need final scrutiny.
- Added a borrowed-property-array survival test, plus byte-level octal validation.
  Eval double-quoted strings now decode up to three octal digits and retain arbitrary
  byte values through TokenKind/EvalConst ByteString rather than UTF-8 expansion.
  Single-quoted behavior is unchanged. Hex/Unicode escape debt was not claimed fixed.
  New debug key tests exposed the missing NUL decoding; they now PASS.
- R15 remains OPEN: the frozen oracle warns for overflowing octal escapes such as
  \\400 and \\777. Values wrap correctly (ff00ff test PASSes), but eval does not yet
  emit those parse warnings. Warning replay must respect cached parses and reporting
  masks; do not classify byte-value correctness as full diagnostic parity.
- Final native test_datetime_eval_debug_ group: 7 PASS, including both clean-heap
  tests, user property order/visibility, uninitialized native date, hook priority,
  by-ref array survival and binary octal bytes. The five-target assembly fixture was
  extended with a by-ref __debugInfo and property read; all five targets PASS.
- Unit filters PASS: lexer::tests 28, parser::tests 216, native_ 58, print_r 3,
  var_dump 3 (overlap). Existing multiline instruction comments were moved to their
  instruction-call lines without changing assembly; alignment and diff checks PASS.
- A batch hit disk exhaustion during linking, separate from the reference SIGSEGV.
  Compressed 40 resolved generated main.s target-test artifacts under the dedicated
  /tmp tree; every gzip integrity check PASSed. Sources are recoverable from .s.gz.
  Re-ran the complete native debug group successfully afterward. No broad cleanup.
- update-builtin-docs: exporter rebuilt warning-free after removing stale imports;
  987 builtins, 190 classes, 1089 constants, 1869 rendered pages. Docs audit 0 errors,
  site validation 1890 pages. Newly added debug_hooks.rs and eval_method_results.rs
  are source files that must be included in future locks/staging.
- R14 is not universally certified: null/invalid-hook diagnostics, unknown/dynamic
  return ownership, identity presentation and remaining initialization/reference
  edge cases still need review. R03/R09/R13/R14/R15 and final parity gates, Astra,
  squash, push and replacement PR remain open.

### 2026-09-08 continuation: native debug projection and output ownership

- Added an AST-only private __elephc_debug_properties method to DateTime and
  DateTimeImmutable. It clones native serialization logic, returns boxed Mixed
  storage, and yields an empty snapshot for uninitialized dates. It is included in
  eval-reachable lowering and invoked with an explicit native base-class scope,
  not virtual dispatch or a user __serialize override.
- New core/native_date_debug.rs collects native date fields and user properties.
  Native user declarations are walked base-to-derived; reads enter the declaring
  class scope, preserving private/protected visibility. Static/virtual properties
  are excluded. Dynamic public properties follow declared properties. Native date
  keys replace public collisions without moving their existing position.
- Added explicit owned_value metadata to debug properties and removed their Clone
  derive. Both renderers release acquired projection values after rendering and
  on errors; pre-existing borrowed property paths remain borrowed. Reflection flag
  decoding is exposed through a helper instead of constants (exporting constants
  caused ambiguous glob imports and was reverted).
- The initial native invocation failed because the private snapshot used ordinary
  dispatch. Supplying Some(base) bridge scope fixed it. Temporary DATE_DEBUG tracing
  was removed. The original combined AOT/eval format-override regression PASSes.
- User property order/visibility/native date collision tests PASS for both families,
  including a __serialize override that throws if called. Uninitialized DateTime
  output PASSes. Frozen PHP oracle confirms these outputs.
- Strict debug heap initially left 8 blocks / 736 bytes after four print_r calls.
  LLDB proved these were output strings and their Mixed cells, NOT snapshots.
  print_r(false) and var_dump now release output cells after echo, including error
  paths; print_r(true) still transfers its string. The strict snapshot heap test
  now PASSes clean. Reproducer: /tmp/elephc-datetime-binding.yhuLdd/r14_debug_heap.php.
- Unit checks PASS: native_date_debug 2, print_r 3, var_dump 3. git diff --check PASSes.
- R14 remains OPEN: new test_datetime_eval_debug_honors_user_debug_info is RED,
  printing native date fields instead of the hook's custom=7 table. Hook handling
  must precede the default native projection. Oracle confirms both print_r and
  var_dump honor __debugInfo; a null result yields an empty table with deprecation
  in PHP 8.5. Inspect returned-array ownership rather than assuming all legacy
  method-return paths own their cells. Also retain visibility/initialization/identity
  edge cases in the final debug audit; current positive coverage is not universal.
- update-builtin-docs completed: exporter warning-free, 987 builtins, 190 classes,
  1089 constants, 1869 rendered pages; docs audit 0 errors, 1890 pages validated,
  EIR boundary 0 structural errors. The two native_date_debug Rust files are NEW
  untracked source and must be included in any future source lock/staging.
- No task process remains active. R03/R09/R13/R14 and final gates, Astra, squash,
  push and replacement PR publication are not complete.

### 2026-09-08 continuation: native date_diff and debug reader overrides

- Completed the interrupted date_diff bypass wiring in AOT and eval. The read-only
  wrapper inventory is now NATIVE_PROCEDURAL_READS/native_procedural_read_method,
  includes __elephc_date_diff, and is included in eval-reachable method lowering.
  Wrapper parameter names come from the public PHP signature, preserving named
  baseObject/targetObject arguments. Receiver diagnostics also use that signature.
- Found internal virtual reads in the active timelib diff AST. Both date families
  now read the target timestamp through the native wrapper, microseconds through
  existing private accessors, and timezone through the native timezone getter then
  its base getName. Target getter overrides must not affect native diff.
- A direct timezone_name_of experiment produced incorrect intervals (29 vs 2 days,
  0 vs 1 total day) and was removed from diff. Keep the normalized native timezone
  getter path; the experiment is not a validated replacement for it.
- Three procedural_diff tests PASS in AOT/eval: mutable/immutable diff overrides,
  named args, target getters that throw, fractional seconds and first-argument error.
  Frozen PHP oracle confirms the expected diff values and baseObject diagnostic.
  Existing getter filter 5 PASS; historical date_diff filter 2 PASS, including the
  400-iteration temporary-memory regression. Five-target emission PASS after diff
  changes (before the subsequent debug-reader edits).
- The remaining self-format calls were in native debug/print_r helpers. Both generated
  AST variants now use the native format wrapper; corresponding test-only reference
  models were updated. Audited-model comparison PASSes after these edits. Production
  never parses these references: parent cfg(test) gates were verified and documented.
- AOT debug override regression PASSed: four native date fields printed and explicit
  format calls still used overrides. Frozen PHP oracle confirms that behavior.
- Extending that regression to opaque eval exposed R14: eval prints the two native
  subclasses as objects with zero properties. The combined test remains RED (4 date
  strings instead of 8), deliberately preserved. This is not an override dispatch
  failure: eval_debug_object_properties falls through to object_property_len/public
  properties for native objects; native DateTime pseudo-properties are not supplied.
  Next inspect that shared debug-property bridge for both print_r and var_dump,
  including property ownership, initialization and subclass behavior.
- update-builtin-docs completed: warning-free exporter, 987 builtins, 190 classes,
  1089 constants, 1869 rendered pages; docs audit 0 errors, 1890 pages validated,
  EIR boundary 0 structural errors. git diff --check PASSes. No task process remains.
- R03 remains open for mutators and interval/timezone aliases, with receiver-alias
  ownership distinct from these read-only methods. R09/R13/R14, exhaustive gates,
  final Astra, squash, push and replacement PR publication remain incomplete.

### 2026-09-07 continuation: owned opaque eval returns and strict heap closure checkpoint

- EvalControl::Return carries EvalExprResult, preserving the producer's ownership.
  Program execution guarantees an owned value at the opaque eval ABI boundary;
  known owners transfer, while borrowed/unclassified values acquire a separate owner.
  Nested eval expressions recognize that producer contract too.
- A lexical owned_program_returns mode acquires pending borrowed program returns
  before finally can unset/reassign their source. The statement-body helper restores
  the previous mode on every Result path. Function, method, closure and include bodies
  explicitly retain their legacy return mode; this is not a universal callable-return
  ownership conversion.
- Finally cleanup releases owned pending returns, not borrowed scope cells. The
  native regression returning a locally created string while finally unsets it and
  allocates a replacement PASSes with a clean heap and the original result intact.
- EIR marks opaque eval LanguageConstructCall values Owned and consumes that explicit
  producer contract when inserting releases. Other language constructs do not gain
  a blanket owning classification. Literal eval's existing paths are unchanged.
- Strict native date-factory heap regression now PASSes with a clean heap: the last
  implicit null cell is released. Re-ran the complete `test_datetime_eval_native_`
  filter after lexical-mode changes: 3 PASS (strict heap, non-accumulation, all five
  supported targets' assembly emission).
- Three native `test_datetime_eval_result_ownership` tests PASS. They include five
  discarded-result bodies (implicit null, explicit null/int/string and borrowed scope
  value), each repeated 20 times with a clean heap, plus finally override/source-unset
  checks. Unit filters PASS: expression_ 22, control_flow 30, dynamic_calls 41, core:: 40.
- Two older exception tests assumed that the first release was the exception. Earlier
  constructor-argument cleanup now releases its string argument first. Tests now
  require exactly one released exception object instead of that incidental ordering.
- R09 remains open for the final ownership audit: unclassified/nullable/union producers
  and legacy function/include returns still require scrutiny. In particular, inspect
  legacy expression-statement cleanup and fresh constructor/container result metadata;
  this clean benchmark does not establish universal eval ownership compliance.
- R03 still needs procedural diff/mutator/interval/timezone coverage beyond the four
  native getter wrappers. R13 omitted-initial AOT array_reduce remains unimplemented.
  Required PHPT/target closure, final Astra, squash, push and clean PR publication
  are not complete. `git diff --check` passes; no task process remains active.

### 2026-09-07 continuation: normalization owner implementation

- EvaluatedCallable::ObjectMethod now records owns_receiver. Only the receiver
  acquired by array_get sets true; lexical special-class receivers, stored closure
  targets and Closure::call/__invoke descriptors remain borrowed.
- Consuming finish/release helpers prevent reuse of a completed normalization owner.
  They release on normal completion and errors, transfer the owner for a result
  cell alias, and preserve original errors. A pending throwable equal to the receiver
  also receives the owner (throw $this), using a non-mutating context getter.
- Wired all inventoried owner boundaries: call_user_func/array, dynamic invocation,
  is_callable, map/filter/reduce/walk, user sorts, iterator_apply, regex callback
  replacement and curl. Curl cleans partial frame construction and terminal pins;
  only one matching pin transfers to a parked throwable. Closure::fromCallable
  transfers acquisition into its stored target and cleans validation/allocation errors.
- Added direct tests for owned versus borrowed targets, returned aliases, pending
  throwable transfer, probe cleanup and validation-error cleanup. Final curl-enabled
  unit filters PASS: callable 45, array_ 87, dynamic_calls 41, curl_ 17 (overlap).
- Native `test_datetime_eval_`: 5 PASS, strict heap regression still FAILS, now
  only 1 block / 48 bytes (previously 3 / 112); exact stdout. Passing cases include
  the new result/Closure transfer regression after source callback-array destruction,
  source-length/source-return checks, non-accumulation and five-target emission.
  Implicit eval result ownership remains open; no zero-leak closure claim.
- Wider array tests exposed an independent shared-contract mismatch: array_reduce
  required three arguments despite its null default. Frozen PHP oracle gives
  required count 2 and named two-argument result 13. Catalog minimum is now 2;
  all eval array tests pass.
- R13 is NOT finished: the new native positive regression
  `test_array_reduce_omitted_initial_value` exposed an args[2] checker panic.
  The checker now produces an explicit unsupported-AOT diagnostic instead. Its
  integer-carry lowering still requires three operands and an Int callback result;
  true omitted-initial/null/Mixed carry support must be implemented, with target
  and callback ownership coverage. Do not substitute zero for PHP's initial null.
  The positive test remains intentionally present and unfulfilled; it was last
  run before the defensive diagnostic (panic), not claimed green after that change.
- update-builtin-docs completed after the defensive checker change: exporter build
  warning-free; 987 builtins, 190 classes, 1089 constants, 1869 pages rendered;
  builtin audit 0 errors, site 1890 pages valid. Generated artifacts remain part of
  the dirty worktree and must be included when publication is eventually authorized
  by the completed gates. Final Astra/squash/push remain pending.

### 2026-09-07 continuation: normalized callback receiver coverage

- Added two focused FakeOps regressions in native_argument_temporaries.rs:
  `callable_receiver_temporaries_release_after_validation_error` and
  `callable_receiver_temporaries_release_after_probe`. Both reproduce the bug:
  receiver release count is 0 instead of 1. The caller's callback array must
  remain untouched. Command: `cargo test -p elephc-magician --lib
  callable_receiver_temporaries -- --test-threads=1` (0 pass, 2 fail).
- Do not release every EvaluatedCallable::ObjectMethod unconditionally:
  eval_array_callable owns the receiver reference fetched by array_get, while
  eval_special_class_array_callable borrows lexical `$this`,
  eval_closure_object_target_callable borrows a stored closure target, and
  method_dispatch constructs borrowed descriptors for Closure::call/__invoke.
  The enum currently has no ownership field, so these paths are indistinguishable.
- Full normalization consumer inventory includes call_user_func/array, direct
  dynamic invocation, is_callable, array_map/filter/reduce/walk, user sorts,
  iterator_apply, preg_replace_callback, curl callbacks and Closure::fromCallable.
  Invocation helpers borrow &EvaluatedCallable and some consumers reuse it;
  cleanup must occur at normalization-owner boundaries, not on every invocation.
- Next implementation: explicit owned-receiver metadata (only array_get acquisition
  owns), error cleanup during validation/argument evaluation, terminal consumer
  cleanup with receiver-result alias transfer, and explicit transfer when a closure
  stores the target. Cover all consumer boundaries; do not stop at call_user_func.
  Preserve native/special-class/visibility dispatch fields and original error status.
- No production callback change yet, so the builtin-doc workflow was inspected
  but no regeneration was needed in this checkpoint. No task process remains active.
  Native strict heap remains last measured 3 blocks / 112 bytes; no closure claim.

### 2026-09-07 continuation: early eval string storage and exact residual owners

- LLDB allocation tracing proved the remaining source allocation was a late
  `__rt_mixed_cast_string` in a Str-typed LoadLocal whose final frame slot had
  widened to Mixed. The frontend `--emit-ir` view did not expose this backend
  conversion. Trace script: `/tmp/elephc-datetime-binding.yhuLdd/trace_source_alloc.lldb`.
- A proposed Str scope-reload cleanup made no difference: all synchronized locals
  in this reproducer use Mixed. That six-line change was REVERTED; do not count
  it as a landed fix or repeat that hypothesis without another reproducer.
- Checker `mark_eval_barrier` now records ordinary visible Str locals in
  `boxed_string_locals` before widening the environment. EIR sees their boxed
  storage from the first store, so the previous turn's explicit temporary-string
  ownership and borrowed-builtin stabilization can release the source correctly.
  Reference aliases, by-ref params, statics, globals and externs keep existing paths.
- Strict native-factory heap test improved from 4 blocks / 640 bytes to
  3 blocks / 112 bytes, with exact stdout. It still FAILS the unchanged zero-leak gate.
- New source-length regression PASSes: padding opaque eval source with 16 versus
  8192 spaces does not increase retained runtime bytes. This is a scoped regression,
  not heap closure. Both `test_datetime_eval_source_cleanup` tests PASS; all four
  `test_datetime_string_local_` tests PASS. `git diff --check` passes.
- Extracted the exact native-factory PHP fixture to
  `/tmp/elephc-datetime-binding.yhuLdd/r09_exact_factories.php`, rebuilt with
  --heap-debug --keep-symbols and inspected its live heap in LLDB:
  * raw factory object: payload 8 bytes, refcount 1;
  * its Mixed object cell: payload 24 bytes, refcount 20;
  * implicit null result: payload capacity 32 bytes, refcount 1.
  Including 16-byte headers these are exactly 112 bytes. No source string remains.
- Next: release/transfer the retained normalized object-callback receiver after
  invocation without breaking escaping Closure::fromCallable receivers or methods
  returning their receiver; establish owned eval return ABI using expression-result
  ownership through EvalControl, including borrowed variables and finally overrides.
- No task process remains active. Final review, full parity gates, squash and push
  remain pending.

### 2026-09-07 continuation: borrowed builtin strings and eval source arguments

- Shared registry EIR lowering now persists a borrowed string result before
  releasing an owned argument it may reference. This covers interior substr
  slices where transferring the backing allocation's owner is not valid.
  Uses target-neutral Acquire, whose Str backend calls str_persist.
- Added a clean-heap regression for repeated/nested substr on a string local
  widened by unset. All four `test_datetime_string_local_` tests PASS; all 27
  `substr` tests PASS, including existing property-assignment heap regressions.
- Eval source arguments now use ReturnArgAlias::None: the parse cache owns its
  Vec source key and Arc<EvalProgram>, so returned runtime cells cannot borrow
  the temporary source allocation. This does NOT establish ownership of the
  eval result itself, which can still borrow a scope value.
- New `test_datetime_eval_source_cleanup_preserves_return_values` PASSes for
  both a literal string return and a scope-variable return after source replacement.
- The first string-stabilization change alone increased the strict native-factory
  fixture to 5 blocks / 1168 bytes. Adding eval source cleanup brought it back to
  4 blocks / 640 bytes, with exact stdout. No net reduction in this fixture yet:
  the original remaining source lifetime, implicit null result, and callable
  receiver ownership still need closure. Do not claim the source leak is fixed.
- The standalone isolate's `--emit-ir` output currently shows a Str local load,
  not the earlier Mixed-to-Str cast; reconcile actual final pipeline/storage
  before assuming that earlier cast hypothesis explains the residual owner.
- Compressed the five generated main.s files under
  `elephc_eval_string_return_targets_72468_ThreadId(5)_{1..5}`; gzip integrity
  checks PASS. Sources remain recoverable from main.s.gz. Disk availability
  also changed due external activity; do not attribute the full gain to compression.
- No task process remains active. Final Astra, PHPT/target closure, squash,
  push and PR publication remain open.

### 2026-09-07 continuation: process-global ownership validation

- Validated the pending argc/argv fixes: process globals now use the same Mixed
  storage as ordinary global reads; fresh argv arrays transfer ownership into
  their boxes; eval global reload retains before replacing the previous owner;
  request cleanup includes implicit eval argc/argv globals.
- Added native-global and repeated opaque-eval mutation regressions in
  `tests/codegen/scalar_strings.rs`. All four `test_argv_` tests PASS.
- The three `datetime_eval_native` tests give two PASSes (non-accumulation and
  five-target assembly), one expected remaining failure: strict factory heap
  regression now reports 4 blocks / 640 bytes, down from 8 / 1040; stdout is exact.
- Rebuilt the mode-zero isolate with symbols and inspected its live heap in LLDB.
  Exactly two blocks remain: 512-byte raw eval-source string, refcount 1, and
  24-byte Mixed null result cell, refcount 1 (568 bytes including headers).
  The previous argv arrays and filename strings are no longer live.
- Next ownership boundary: explicit eval return currently forwards a raw expression
  cell, while implicit return allocates null. Establish a coherent result ownership
  contract before releasing discarded LanguageConstructCall results. The substring
  source cast also needs cleanup after its borrowed slice is consumed. Object
  callback receiver lifetime remains separately open in the full factory fixture.
- `git diff --check` and assembly-comment alignment on the three changed production
  files PASS. No task process remains active. Final audit, squash, push and new PR
  remain pending; no closure claim or relaxed heap assertion.

### 2026-09-07 continuation: evaluated-result ownership and callback names

- Added `EvalExprResult { value, owned }`. Instance-method evaluation propagates
  the result ownership from the actually selected native signature, preserving
  private-shadow/visibility/magic dispatch. Non-null concrete scalar/object native
  ABIs prove fresh boxes; Mixed, nullable/union, container and unclassified paths
  remain conservative. Source argument aliases transfer their owner into the result.
- Echo, conditions, unary/binary expressions, ternaries, null-coalescing and
  suppression now preserve and consume the established result owner. Legacy raw
  evaluation entry points remain available for consumers not yet migrated.
- The strict native date-factory heap regression improved from 208 to 48 blocks
  when the 80 format results were released. Output remained exact.
- Callback array normalization now releases fetched method-name cells after copying
  their bytes to Rust; static receiver-name cells are also released. Error paths
  release fetched receivers when normalization cannot return a callable. Object
  receivers still carry their existing retained reference for callable/closure use.
- The main heap regression is now RED at 8 blocks / 1040 bytes, not complete.
  Do not call these eight blocks harmless: receiver references can accumulate even
  while the number of retained objects stays constant. Investigate normalized
  object callback lifetime and the remaining request/scope owners.
- Passing checks: expression ownership 4; control_flow 30; method_arguments 26;
  classes 129; native_ 49; callback name cleanup 2; dynamic_calls 41 (overlapping
  filters, not unique totals). Native return non-accumulation and five-target
  emission PASS. The stale unknown-named-argument unit expectation was corrected
  to assert catchable Error plus exact message, verified with the frozen PHP oracle.
- A proposed extra boxed-string mark at every eval barrier did not change the
  reproducer and was reverted; do not treat that hypothesis as a validated fix.
- `update-builtin-docs` completed: exporter built warning-free with curl; registry
  contains 987 builtins, 190 classes, 1089 constants; rendered 1869 pages; module
  sections/comparison regenerated. Builtin audit: 0 errors; site validation: 1890
  pages; EIR boundary inventory: 597 registry AOT builtins, 0 structural errors.
  Generated DateTime compatibility status remains Partial, not closure proof.
- `git diff --check` passes. No local task process remains active. Final Astra,
  squash, push and replacement PR publication remain pending.

### 2026-09-07 continuation: evaluated arguments and native staging

- Added one per-expression source-owner ledger around method, static, dynamic,
  nullsafe and constructor calls. It tracks proven scalar temporaries, preserves
  source-order/named binding, cleans evaluation/invocation errors, and transfers
  an owner when the result is the original argument cell. Ownership is not copied
  onto forwarded/cloned EvaluatedCallArg records. Containers/spreads remain open.
- Six ownership tests PASS; regression filters PASS: method_arguments 26,
  classes 129, native_ 48 (these sets overlap; do not sum as unique tests).
- The initial native total did not change because fetched argument cells still
  had refcount 2 after invocation. A temporary native diagnostic confirmed the
  new ledger was running against the current archive, not stale code. All trace
  code, environment checks and temporary unsafe header reads were then removed.
- New `src/codegen/eval_arg_ownership.rs` centralizes boxed-index cleanup and
  release of staged by-value scalar cells where the return cannot reuse their box.
  Method and constructor emitters use it on both architectures. Reference and
  unproven Mixed/container-result paths keep the conservative policy.
- By-value string arguments now use the borrowed byte-view API while their
  argument array remains alive. By-reference strings keep their mutable copies.
- Native date-factory fixture: 528 -> 288 after index/fetched-cell cleanup, then
  288 -> 208 after eliminating detached by-value argument string copies.
  Latest strict result remains RED: 208 blocks / 10632 bytes, exact PHP stdout.
  The native return non-accumulation test and five-target emitter test PASS.
  Three eval_arg_ownership emitter unit tests PASS. `git diff --check` is clean.
- Next: inspect owned eval expression results consumed by echo, callable descriptor
  lifetimes, and request-end scope owners. Do not classify all dynamic or Mixed
  method returns as owning without proving the actual return contract.
- No task build is active. R09, final Astra approval and publication remain open.

### 2026-09-07 continuation: stable string storage and borrowed byte views

- R12 is locally fixed: `boxed_string_locals` generalizes the former increment-only
  contract to plain string locals requiring null-store unset. Boxing is known from
  the first store; references/statics/extern/global storage keep their existing
  paths. Loop overwrite cleanup uses final slot storage rather than stale Str loads.
- By-value string call results no longer inherit borrowed-object return suppression.
  Actual by-reference returns retain their existing policy. Generic string casts
  are persisted at return; owned source copies are released after persistence, and
  MixedCastString cleanup uses the validating heap-string release path.
- Three strict clean-heap regressions PASS: repeated string reuse after unset,
  by-value typed string parameter/return alias cycles, and mixed scalar/string
  casts including a returned value surviving another scratch-buffer conversion.
  The complete `locals_retype` module passes 134 tests.
- Found and fixed the common eval metadata leak: the C byte-view API promised a
  borrow but called mixed_cast_string, which allocates for tag 1. Both emitters
  now return the original string bytes for tag 1 and retain scratch conversion
  for non-string scalars. This also fixes the direct timezone-setter API consumer.
- Native eval regression results: the non-accumulation test for instance/static
  string AND DateTime return factories now PASSes; five-target emission PASSes.
  The original full eval date-factory heap test remains RED with correct stdout:
  528 blocks / 25968 bytes, down from 948 / 45960. R09 is still OPEN.
- The emitter unit regression passes for macOS ARM64, Linux ARM64 and Linux x86_64,
  checking the borrowed-string branch and balanced x86 frame. `git diff --check`
  passes. No local task build remains active.
- Resource maintenance: removed six generated test executables/object files only
  (62 MiB); PHP/assembly sources were preserved. Ten generated target main.s files
  are retained as lossless main.s.gz archives, all checked with gzip -t. Observed
  free disk is 9.3 GiB after cleanup and concurrent external activity.
- Next R09 work: expression-result cleanup for eval native method calls, literal
  argument ownership, detached native string argument staging, callable descriptors,
  and request-end eval scope owners. Preserve the strict zero-leak main regression.

### 2026-09-07 continuation: direct eval method return ownership

- Direct instance/static method bridges now adopt native owned strings, matching
  callable invokers. Object adoption uses the existing EIR proof for the selected
  implementation; runtime-helper fallbacks remain conservative. Both architecture
  emitters use this policy.
- `test_datetime_eval_native_string_return_all_target_assembly` PASSes for all five
  supported targets. The main native date-factory eval fixture still produces its
  exact output but remains RED: 948 blocks / 45960 bytes, down from 1188 / 74144.
  This is 240 fewer live blocks, not final ownership closure.
- New per-call growth regression remains RED and must be preserved:
  `test_datetime_eval_native_string_returns_do_not_accumulate` now exercises both
  string and DateTime return factories, instance + static. One iteration leaves
  16 blocks; twenty leave 206. Earlier string-only version was 11 versus 106.
- R12 reproducer `/tmp/elephc-datetime-binding.yhuLdd/pure_string_returns.php`
  reproduces 59 blocks / 2832 bytes WITHOUT eval, using two substring-returning
  methods, assignment/echo/unset, repeated 20 times. The mixed AOT/eval comparison
  probe `eval_string_boundary.php` gives 65 AOT / 108 eval blocks for its two paths.
  Do not attribute the entire new regression exclusively to the eval bridge.
- EIR evidence for R12: the final local slot is Mixed after unset; an earlier
  overwrite cleanup still loads that slot as Str. Native method return is
  `runtime.substr -> str_persist -> return`. `store_value_to_raw_local` ALREADY
  adopts Acquire-produced strings via `value_can_own_mixed_box_source`; do not
  blindly add a second adoption/release. `substr` itself returns a borrowed slice.
  The exact allocation/refcount explanation for all 59 blocks remains unproven.
- Current isolate probe (before the new direct-method boxer change): baseline 6;
  direct native date creation 8; dynamic function 30; static callback 152; instance
  callback 236. Adding format adds 208 blocks to each 20-call case. Rebuild that
  probe before claiming post-change counts.
- No local process remains live at this checkpoint; final review/publication are
  still pending, and no failing test was removed or ignored.

### 2026-09-07 continuation: shared native argument arrays

- Added `RuntimeValueOps::argument_array`: both registered-function invokers and
  native method/static/callable runtime adapters now use the same builder.
  It releases synthetic index cells on success and failure, releases a partial
  array on error, and never consumes the borrowed argument owners.
- Two focused cleanup tests PASS; the broader `native_` interpreter filter passes
  44 tests, including those two. Native construction is also compiled through the
  runtime adapter path, which is excluded from the fake-runtime unit build.
- The unchanged native date-factory eval fixture still produces its exact expected
  output but remains RED: 1188 live blocks / 74144 bytes, down from 1268 / 77272.
  Eighty index cells are demonstrably reclaimed. R09 remains open.
- Next argument-lifetime work must account for `EvaluatedCallArg` cloning and
  forwarding: 25 constructors in 15 source files, plus recursive method dispatch.
  A copied ownership flag followed by cleanup at every forwarding layer would
  release the same owner twice. Preserve one cleanup boundary per evaluated source
  argument and transfer aliases when a callee returns that argument.
- No build remains active. No final review, squash, push or PR publication yet.

### 2026-09-07 continuation: eval expression temporaries

- Added `expressions/temporaries.rs`: proven literal/operator temporaries are
  released after a borrowing consumer, on both success and error. Variable loads
  and function-like results are deliberately not presumed owning.
- Binary operands, short-circuit conditions, unary operands (including synthetic
  zero cells), `if`/`while`/`do`/`for` conditions, and echo literal/operator results
  now use the shared cleanup. Existing eval destructor-aware release is reused.
- Five new `expression_temporaries` unit tests PASS; 30 existing `control_flow`
  tests PASS. Tests cover borrowed-variable survival, no duplicate releases,
  right-operand failure, short-circuit evaluation, unary temporaries and echo.
- Native date-factory eval fixture remains RED, unchanged: before the echo cleanup
  checkpoint, 1428 blocks / 84776 bytes; after echo cleanup, 1268 blocks / 77272
  bytes. Both produce the exact expected date output. Thus 160 additional blocks
  are demonstrably reclaimed, but this is not R09 closure.
- Next concrete source points: `EvaluatedCallArg` contains name/value/ref_target,
  but no ownership carrier; `build_native_function_arg_array` creates index cells
  without releasing them after array_set. Method/native argument binding and
  returned string cells consumed by echo still need ownership-safe cleanup.
- No active local build remains at this checkpoint. `git diff --check` passes.

### 2026-09-07 continuation: dynamic object argument guards

Latest checkpoint: the 40-block identity-wrapper leak below is FIXED. Actual EIR
showed the callee parameter widened to Mixed despite caller-side signature facts.
Object-to-Mixed cleanup now consults the compiled by-value parameter ABI or proves
that every return is a fresh MixedBox of an object. Ambiguous callees still retain
the conservative policy; this is not a blanket change to all Mixed return contracts.
`test_datetime_object_`: 5/5 PASS (four native clean-heap regressions plus one test
emitting all five supported targets). The original failing test is unchanged and
now clean. `git diff --check` passes. Broader R09 eval/callable residuals remain open.
The seven existing non-eval `object_return_heap` regressions also PASS (23.90s).
The known-red `test_datetime_eval_native_factories_object_return_heap` was explicitly
excluded from this non-eval regression run, not ignored or removed; its eval
temporary-ownership remediation remains required before final closure.

- Added a shared Mixed-to-object guard predicate, accepted only at direct
  by-value user-function binding sites, with matching post-planner EIR checks.
- Added a BorrowedTemp alias API for synthetic AST diagnostics; storing a borrowed
  alias must not release the original owned SSA value. Direct date wrappers now
  reuse the same argument-type-name AST builder.
- The five `test_datetime_procedural_getters` tests pass, including timezone and
  offset getter overrides on both date families across opaque eval.
- The initial argument-order/type-error regression passes. PHP oracle confirms
  the diagnostic type names. PHP's additional `called in FILE on line N` suffix
  is not yet represented by this lowering context and remains an explicit gap.
- The owned argument rejection heap regression remains RED: 20 identity-wrapper
  calls returning a Mixed object leave 40 blocks / 12280 bytes. Preserve this test;
  investigate parameter-to-return ownership rather than weakening its expectation.
- Final focused evidence: named-argument source order, invalid argument type names,
  and cleanup of other owned arguments PASS with a clean heap; the fresh-Mixed-
  factory rejection case also PASSes with a clean heap (20 iterations). The
  identity-wrapper case still fails with exactly 40 blocks / 12280 bytes. This
  isolates the residual to the object-parameter/Mixed-return transfer, rather
  than an unconditional leak in the new guard's diagnostic path.
- Source pointer for the next fix: `release_owned_call_arg_temporaries_with_signature`
  in `src/ir_lower/expr/nullable_method_calls.rs` suppresses a raw Object argument's
  release when a Mixed return may wrap it and pointers cannot be compared. Prove
  whether the callee retains versus adopts that object before changing this rule;
  do not globally assume every Mixed return owns a separate reference.
- No final Astra review, documentation regeneration for the latest getter aliases,
  squash, push, or publication has been performed at this checkpoint.

Report: `/tmp/elephc-astra-datetime-post-rebase-audit.md`, GPT-6 Astra LOW,
verdict CHANGES_REQUIRED, ten source findings plus one metadata finding.
HEAD was `c423b1d56dc2a3cba712688b0fde6540aeeb4b3d`, on main base
`0499d54914dba2ee612b6307cf3355266994428d`. Source hash was
`178170104ae833f7d937a582b819998827304ddcbc1dee69195d9d649d5e9461`.
Parent read the complete final report; source freeze is lifted for remediation.
No final approval or publication is claimed.

## Invariants

Production preludes use AST, not embedded PHP parsing. Supported language profiles
are minor selectors with stable patch-zero spellings and no development suffix;
the exact php-src development revision is only the audit oracle. All supported
targets remain first-class. Preserve the dedicated worktree and backup branch.
Run focused validation sequentially with the existing single-job target directory.

## Progress

### Current implementation checkpoint

- R03 getter wrong-receiver diagnostics implemented in AST: reserved wrappers now
  throw TypeError with procedural function/argument identity, native class names,
  and PHP null/false/true/int/float spellings. Eight invalid kinds are checked for
  both date_format/date_timestamp_get, direct and opaque eval; exact fixture also
  executed against frozen PHP and matched all 32 expected message lines.
- Initial direct bad-receiver heap probe left 22 blocks / 1560 bytes. Removed an
  unnecessary owned gettype temporary, read the borrowed wrapper parameter during
  diagnostics, and split exception construction from throwing so the retained
  receiver is released before control leaves the helper. The reusable exception
  builder preserves existing spread-error behavior; its emit wrapper is unchanged
  semantically for previous callers.
- FOUR test_datetime_procedural_getters* regressions now PASS (session 21217,
  47.39s), including a clean-heap invalid-receiver case. Standalone diagnostic probe
  with getMessage output also clean: 118 allocs / 118 frees, zero live blocks
  (session 44747). Previous sessions 26285, 73170 and 77241 terminal. Hygiene passes.
  This covers the two implemented getters; other aliases/mutators and R09 eval
  temporaries/mixed return contracts still require work. No final audit/publication.

- R03 first two getters implemented: native_procedural.rs constructs private static
  AST wrappers for date_format and date_timestamp_get by cloning native signatures.
  No PHP parsing added. Shared NATIVE_PROCEDURAL_GETTERS metadata selects exact
  DateTime/DateTimeImmutable EIR calls and records independent return storage.
  AOT name rewriting, opaque-eval aliases and native-method retention use wrappers.
- test_datetime_procedural_getters_bypass_user_overrides PASSES for mutable and
  immutable subclasses, direct and opaque eval, while explicit methods remain virtual.
  Initial temporary-argument heap probe leaked 100 blocks: EIR omitted release of
  date_format's Mixed argument due Unknown alias summary. Shared reserved getter
  metadata now provides ReturnArgAlias::None. Added permanent heap regression;
  BOTH test_datetime_procedural_getters* tests PASS, clean heap (session 6533, 24.06s).
- update-builtin-docs exporter and full regeneration/audits completed successfully
  for the new wrappers; last alias-only metadata centralization changes no public
  signatures. Final source-frozen docs workflow still required before publication.
  Sessions 95832, 80597, 25619, 15944, 6533 terminal. Diff hygiene passes.
- R03 remains open for other procedural aliases/mutators and exact wrong-receiver
  TypeError diagnostics (current native-wrapper no-match still uses the generic
  invalid-DateTimeInterface error). New untracked native_procedural.rs must be
  included in the final source hash. R09/eval temporary leaks and final gates remain.

- R11 source/local linkage fix verified: added TZ to the existing embedded bridge
  relation (alongside Crypto/Iconv/Phar). Bridge resolution propagates forced
  whole-archive requirements to the provider, including exact archive inputs.
  Tests cover standalone preservation, duplicate suppression and forced named/exact
  provider selection. IMPORTANT correction: earlier --lib embedded run (4 tests)
  did not exercise linker module tests because linker is in the binary crate.
  Correct --bin elephc embedded run passes EIGHT tests (session 72757, 0.41s).
- Shared relation lookup is also used by normalization, eliminating the intermediate
  unused-helper library warning. CLI rebuild now warning-free (1m54s). Recompiled
  native probe non-incrementally with ordinary linking and with --with-tz: both
  link and execute without duplicate-symbol warnings. Both keep identical existing
  eval heap residue 458 blocks / 30248 bytes, so no unrelated heap closure claimed.
  Sessions 28554, 98639, 10828, 72757 and 89994 terminal. Diff hygiene passes.
  Linux/release artifact execution remains part of the final target gate; local
  macOS linkage is not evidence of running that matrix. R03/R09 work stays open.

- Indexed EvalIR arrays now release generated numeric-key cells and fresh literal/
  nested-array element cells after the retaining native setter. Variable/reference
  elements remain borrowed. Key cleanup also runs on evaluation/insertion errors;
  complete partial-array error cleanup and arbitrary expression ownership are NOT
  yet solved. Fifteen array_literal Magician tests pass (session 42808).
  Native mode4 results improve by another 40 blocks over 20 calls: no format
  498 -> 458 (30248 bytes), with format 766 -> 726 (42448 bytes). Output correct;
  strict heap remains red. Session 28364 terminal, native bridges rebuilt with
  CARGO_INCREMENTAL=0. Diff hygiene passes.
- Disk pressure: own debug/incremental cache measured 5.1G with no Cargo/rustc
  active. Forced removal command was refused; non-forced removal of that exact
  generated cache succeeded (session 50590). Sources, binaries, probes and deps
  preserved; available space rose from 848Mi to 4.9Gi. Cache is regenerable.
  Prefer CARGO_INCREMENTAL=0 for subsequent validation to limit regrowth.
- R11 discovered by that non-incremental bridge build: linker warns of duplicate
  elephc_tz_* C exports from libelephc_tz.a and libelephc_magician.a. Example object
  provenance: tz-1427dd... .rcgu.o vs dependency tz-f07799... cgu.2/3.rcgu.o.
  Read-only nm confirms ALL 23 standalone TZ C exports also exist in the current
  macOS Magician archive (comm missing-set empty). Do not silently suppress the
  warning or infer release/Linux coverage from these artifacts. Resolve native
  provider ownership/packaging and validate supported targets before publication.
  No own process remains live; R03 and other R09 cases still open.

- Callback-array temporary cleanup implemented: shared call_user_func* predicate
  now treats EvalExpr::Array as owned alongside Const, while LoadVar stays borrowed.
  Unit callback_array_expression_owns_temporary_container PASSES (session 21188).
  Native rebuilt Magician probe confirms 40 fewer live blocks over twenty instance
  callback-array calls: no-format 538 -> 498 blocks (32008 bytes now); with-format
  806 -> 766 (44232 bytes now). Correct output preserved; heaps remain non-clean.
  Sessions 67866 (exporter), 83263 (docs), 50440 (native probe) all terminal.
- update-builtin-docs completed after the builtin edit: warning-free curl exporter,
  JSON/1869 pages regenerated, module sections/comparison regenerated, builtin audit
  zero errors, site compatibility 1890 pages, EIR target-boundary audit zero errors,
  status/diff inspection and diff hygiene passed. Generated changes remain in the
  existing dirty worktree for eventual PR; no hand-edited generated pages.
  Remaining eval expression/argument leases, R09 mixed contracts, R03, final audit
  and publication are still open. No own active process remains.

- Eval leak isolation now has one binary with nine modes, preserved in
  /tmp/elephc-datetime-binding.yhuLdd/r09_eval_isolate.php (session 69545 terminal).
  Twenty-iteration baseline with no factories: 108 live blocks / 4920 bytes.
  Without formatting: direct native free=150/7664, dynamic free=212/10520,
  static call_user_func=374/26360, instance callback array=538/34622.
  With format: direct=418/19808, dynamic=480/22568, static=642/38592,
  instance=806/46822. All executions succeed with correct dates. These are total
  live blocks/bytes, not deltas; mode argc bookkeeping also differs between runs.
- Source evidence: eval_expr LoadVar returns the visible scope cell as a borrow.
  RuntimeValueOps scalar constructors/comparisons allocate fresh native Mixed
  cells. eval_expr and evaluated-operation helpers return raw RuntimeCellHandle
  without an ownership carrier; several intermediates are not released. Do not
  blindly release every eval result: scope reads and some returned aliases borrow.
  call_user_func's callback-temporary classifier currently recognizes only Const,
  not freshly allocated Array callbacks. This is another concrete cleanup gap to
  reconcile with array-element ownership and callback result transfer.
  No speculative arena or blanket-retain/release fix was applied. No own sessions
  remain live. Next work must cover expression/argument temporary ownership as
  well as native object return policy; R09/eval strict heap test remains red.

- Eval return-policy wiring added: native free-function registration consults
  concrete EIR ownership, and eval string/static/instance callable descriptors
  carry the same proof. Shared analyzer visibility widened only inside codegen.
  This source wiring is NOT a clean-heap validation of all eval native call paths.
- Eighth test test_datetime_eval_native_factories_object_return_heap produces the
  correct 0|0|1|2| x20 but FAILS strict heap (1570 blocks / 91576 bytes). Previous
  seven tests still pass (session 34444 terminal, 63.96s including native bridge
  rebuilding). New test covers registered free calls, dynamic function-name calls,
  static method strings and receiver/method arrays from opaque eval.
- Isolating probe /tmp/elephc-datetime-binding.yhuLdd/r09_eval_native_factories.php
  compares zero vs twenty iterations in one binary, using argc-dependent substr
  to retain opaque eval (leading newline means offsets 0/1 both valid). Baseline:
  25 allocs/18 frees, 7 blocks/880 bytes. Twenty iterations: 14427/12876,
  1551 blocks/90720 bytes. Hence 1544 additional live blocks are invocation-related,
  not merely context initialization. Separate the four call forms and actual native
  method callback vs descriptor dispatch before changing further ownership code.
  Both runs exit 0 with correct output; heap gate remains red. Sessions 44720 and
  39221 terminal; no own live sessions. Hygiene and checked emitter comments pass.

- Concrete callable producers now propagate owned object return proof for explicit
  static FCCs, free-function FCCs, closure descriptors, runtime user-function cases,
  and runtime static-method cases. FirstClassCallableDescriptor carries the proof;
  shared invoker cache already distinguishes owning/non-owning policies.
- New factory test first failed with 4 constant blocks / 296 bytes, while all six
  earlier tests passed. Standalone r09_factory_descriptors.php reproduced identical
  leakage with zero invocations (9 allocs / 5 frees) and 60 invocations (9189/9185).
  EIR releases the array and each newly-stored descriptor, but emit_array_value_type_stamp
  omitted Callable; deep-free supports tag 10 on both ABIs. Added Callable => 10.
  SEVEN object_return_heap tests now PASS with clean heaps (session 38214, 19.11s).
  Sessions 66137, 34410, 33135, and 38214 are terminal.
- Diff hygiene passes. Assembly-comment checker passes new wrapper/metadata/array
  stamp changes; core_closures.rs has 3 pre-existing multiline-call findings outside
  the edited invoker block. Do not report that entire file's checker green.
  Late-bound static dispatch, eval registration return policies and mixed ownership
  paths remain open, as does R03. No final audit, squash or publication yet.

- Virtual R09 extension validated: SIX object_return_heap tests pass cleanly
  (session 2007, 16.43s). Op::MethodCall cleanup resolves every compiled descendant
  of the typed receiver and requires agreement among their concrete EIR return
  ownership summaries. The regression alternates base/override receivers whose
  methods return an owned narrowed argument vs a fresh DateTime. No agreement or
  no proof still means Unknown; differing contracts and within-callee mixed paths
  remain open. Hygiene and assembly comment checks pass; own sessions terminal.

- Static R09 extension validated: object_return_heap filter now passes FIVE tests
  with clean heaps (session 60026, 14.63s). Added static conditional fixture that
  alternates returning the same Mixed argument through the owning object boundary
  and returning a fresh DateTime. Cleanup resolves explicit static/lexical targets
  using existing backend receiver/implementation-owner rules before EIR ownership
  analysis. Late-bound static, virtual, and mixed owned/borrowed return summaries
  remain Unknown and are not closed. Hygiene/comment checks pass; no live sessions.

- R09 latest checkpoint: all FOUR object_return_heap regressions pass natively
  with clean heap (session 62796, 11.52s). Previous cold session 10958 completed:
  callable cases passed, both direct cases leaked; this directly motivated cleanup.
  New object_return_ownership.rs classifies every concrete EIR return as Owned,
  unchanged raw-object parameter Borrowed, or Unknown. Instance invokers share it.
  Direct Op::Call Mixed-box cleanup now releases the distinct box for Owned,
  retains the aliased object then releases its box for proven Borrowed; Unknown
  keeps the conservative old path. No signature-only ownership guess is made.
- All five targets emitted assembly for r09_direct_ownership.php with the borrowed
  transfer branch present (session 94510 complete). This is emitter evidence, not
  Linux/iOS execution. Assembly-comment alignment passes for ownership.rs,
  object_return_ownership.rs and runtime_wrappers.rs; diff hygiene passes.
- R09 not yet fully closed: static/virtual/MAY-return-parameter and inconsistent
  owned/borrowed return paths need extension/proof. Unknown fallback can still
  preserve a box. Non-instance descriptor consumers still have legacy return
  policy. New untracked source object_return_ownership.rs must enter audit hash.
  All own build/tool sessions terminal. R03 and final audit/publication remain.

- R03 regression test_datetime_procedural_getters_bypass_user_overrides added for
  date_format/date_timestamp_get vs explicit overridden methods, direct and opaque
  eval. Frozen PHP executed the exact fixture and confirmed
  1970|override|0|42| repeated twice. Elephc regression not run yet; procedural alias
  rewrite still emits virtual MethodCall for these and several getter/setter aliases.
  No R03 production fix claimed. Current cold build session 10958 remains active
  (Cargo PID 11319, rustc working at elapsed 7m07s); don't restart.

- Added direct R09 harness regressions test_datetime_mixed_argument_owned_object_return_heap
  and test_datetime_mixed_argument_borrowed_object_return_heap through the shared
  assert_datetime_argument_object_return_heap helper. Both exercise 20 true nullable
  Mixed argument temporaries and require clean heap after format/unset. They match
  the running object_return_heap filter, along with the two callable regressions.
  Cold rebuild session 10958 is still live (Cargo PID 11319, rustc active at 3m10s).
  No outcome yet; diff hygiene passes. Do not substitute a signature-only alias
  shortcut for actual owning-vs-borrowed return provenance: a raw parameter callee
  can itself return through an owning Mixed narrowing on some paths.

- Borrowed-callable argument bug: runtime_callable_invoker indexed argument loading
  emits an incref after Mixed->Object unboxing, but associative and ref-cell paths
  in push_materialized_mixed_hash_value_arg / push_loaded_invoker_ref_cell_value_arg
  did not. All paths later release the raw argument owner. Preserved assembly
  r09_callable_return_contracts.s around 140680-140850 shows the mismatch.
  Shared materialized-Mixed argument preparation now acquires unboxed containers/
  objects before dropping a temporary source box; borrowed unchanged Mixed cells
  are retained, fresh source boxes transfer. Ref-cell values use that same helper.
- An external cleanup process PID 86220 removed the complete task target directory
  while no build was active. Verified process ended and directory gone before
  rebuilding; we did not perform that cleanup. Free space rose to 9.8 GiB.
  Cold focused rebuild/test session 10958 is LIVE for filter object_return_heap.
  Do not restart without polling this handle. Source change not yet validated.

- Latest: test_datetime_interface_dispatch_callable_object_return_heap PASSES
  with clean heap (20 invocations) after selecting class_methods. Companion probe
  rebuilt with the same binary STILL fails after first 1970| with bad-refcount.
  Added permanent test_datetime_callable_owned_and_borrowed_object_return_heap,
  not yet run as a harness test; its equivalent standalone probe reproduces failure.
  Sessions 82350 and 92830 are terminal. Owning instance-method return leak fixed;
  borrowed argument/return lifetime and remaining callable surfaces stay open.

- Invoker object ownership implementation underway: RuntimeCallableInvoker carries
  owned_object_return, included in the shared signature/capture cache key. Instance
  method descriptor paths inspect the concrete class_methods EIR body and select
  owning boxing only if every returned value is Owned and return is non-reference
  Object. Other invokers retain their prior behavior, pending separate proof.
  First replay still leaked because lookup mistakenly used module.functions;
  corrected to module.class_methods. Focused heap replay session 82350 is live.
- Companion /tmp/elephc-datetime-binding.yhuLdd/r09_callable_return_contracts.php
  has two same-signature instance methods (fresh DateTime return vs borrowed
  DateTime parameter return), loop-callable invocations and explicit unset. With
  pre-lookup-fix binary it prints one 1970| then bad-refcount fatal. This is an
  additional borrowed-return invocation failure, NOT resolved by the owning branch.
  Must diagnose argument-owner cleanup/boxing order and add a permanent companion
  regression. Static/closure/free-function/eval registration paths still use the
  legacy return policy and are not certified by this instance-method improvement.

- R09/R10 callable leak isolation: one binary probe now selects operations by argc.
  argc=1: date construction/unset clean (141/141). argc=2: interface method/unset
  clean (212/212). argc=3: FCC construction/unset clean (142/142). argc=4: FCC
  invocation leaks 4 blocks / 608 bytes (219/215). All branches compiled together.
  Probe remains /tmp/elephc-datetime-binding.yhuLdd/r10_callable_probe.php.
- EIR of VariadicDate::modify forwards an owned static_method_call result from
  parent::modify. runtime_callable_invoker::emit_boxed_invoker_return uses borrowed
  boxing for Object; __rt_mixed_from_value retains the payload and leaves this
  owned return unconsumed. Its comment explicitly assumes all container/object
  returns borrowed, which is false for this callee. Do not globally adopt object
  returns: the companion R09 borrowed(Object)->Object case needs retaining.
  Invoker metadata/cache currently keys signatures and captures, not result
  ownership. Tie the repair to authoritative return ownership before changing it.
- Added test_datetime_interface_dispatch_callable_object_return_heap (20 iterations,
  clean heap required), not run yet. It captures the known failing owned-object FCC
  case as a non-ignored regression. No Cargo started during this checkpoint; disk
  was 1.6 GiB. No active own sessions remain.

- R10 latest: both test_datetime_interface_dispatch_subclass* tests PASS natively
  (optional args; variadic defaults; FCC; nullable receiver/nullsafe). The first
  extended failure was a premature release of the receiver-owner load during its
  retained store into the concrete receiver temp. EIR showed release v2 in the arm
  followed by release of that same slot at merge. Concrete stores now receive an
  explicit acquired reference without consuming the common owner. No callable
  resolver change was needed: standalone FCC creation already worked.
- Heap-debug probe /tmp/elephc-datetime-binding.yhuLdd/r10_callable_probe.php now
  completes without the previous bad-refcount fatal. It still leaves 4 blocks / 608
  bytes after unset. Replacing advance_variadic_date($date) by direct $date->modify
  produces the SAME 4 blocks / 608 bytes (289/285 allocations/frees vs 290/286).
  This is not a clean-heap gate; investigate the shared callable/return path before
  closure. Probe restored to interface call. All own tool sessions are terminal.
  An unrelated Cargo test is active in another checkout; do not stop or clean it.

- R10 implementation underway in new ir_lower/expr/date_interface_calls.rs:
  branch on date-family classes descendant-first, preserve the receiver owner,
  and lower each selected concrete call through shared argument preparation.
  Optional override regression passes natively. Extended variadic/FCC/nullsafe
  fixture first failed with the old base-ABI arity error. Nullable boxed receiver
  handling is now included, using an acquired box and ReturnBoundaryMixedToObject;
  replay session 54792 is live. Frozen PHP confirms the extended expected output.
  This new source is untracked and must be included in the final audit hash.
  R10 is not closed: extended replay, ownership and target proofs remain pending.

- Latest R08 validation supersedes the older live-session/failure notes below:
  focused mktime codegen tests passed 19/19; nullable-int callable heap regression
  passed; param_binding tests passed 10/10; union return representation unit passed.
  Generated AST declarations match their audited models. Checked bridge assembly
  emission passes for all five targets (not executable Linux/iOS evidence).
  gen_builtins compiled warning-free with curl; registry and 1869 pages regenerated.
  Module sections and PHP comparison regenerated; builtin audit (zero errors),
  site compatibility (1890 pages), EIR target-boundary audit, and diff hygiene pass.
- R10 source inspection confirms candidate dispatch still uses the base signature
  operand count and preloaded ABI arguments. Do not merely relax the equality:
  concrete optional/default/variadic operands must actually be materialized.
- Added test_datetime_interface_dispatch_subclass_optional_override; frozen PHP
  confirms default|1970-01-02|1970-01-02|. Native regression not run yet; R10 unfixed.
  All tool/build sessions are terminal. Disk availability fell from 6 GiB to 3.5 GiB
  during this checkpoint; inspect before another build. No cleanup was performed.

- R08 checked bridge is implemented in crates/elephc-tz/src/mktime.rs. Nullable
  bits 1..5 select defaults from one clock sample; a repr(C) two-i64 result separates
  timestamp from validity. Three Rust tests pass for layout, valid -1 vs failure,
  and coherent clock defaults. Original six-integer raw C helpers stay unchanged.
- Public Mktime/Gmmktime lowering now uses this checked bridge and boxes int/false
  on both architectures; raw RuntimeFnIds dispatch to dedicated integer-only paths.
  Public return ownership is Fresh. TypeSpec now represents False and Union, and
  mktime/gmmktime contracts declare int|false. Converter, docs exporter and parity
  matcher handle those types. Library requirements and runtime-callable eligibility
  now include the public date functions. Native names selected at runtime have a
  new regression, still to run at this checkpoint.
- Removed obsolete private DateTime AST mktime adapters and their injection/model
  wiring after removing the source rewrite. Backup of the removed newly-created
  helper: /tmp/elephc-datetime-binding.yhuLdd/mktime_ast_previous.rs. Frontend registry
  argument preparation remains; late wrappers complete defaults in the checked bridge.
- The strict 40-iteration callable heap regression passed with a clean heap after
  the checked return fix. An 18-test mktime run then found corrupted supplied nullable
  arguments in loop FCC invocations: direct/eval -1 results were correct, but three
  dynamic calls returned huge timestamps. 17/18 passed before that fix.
- Root cause: runtime_callable_invoker could not coerce Mixed to TaggedScalar and
  passed box pointers with the wrong word count. Added nullable_args.rs, explicit
  tagged null defaults, supported scalar coercions, and restoration of BOTH words
  after temporary-box cleanup. test_mktime_minus_one_is_not_false now passes.
  Assembly-comment checks pass for both newly-added emitter files; diff hygiene passes.
- Current extra regression checks a returned user callable's nullable-int inputs
  from strings/floats/bools/null/missing args with strict heap debug. Its first run
  used unavailable get_debug_type; it now uses gettype. Session 61893 is live.
  Broader mktime replay, converter unit tests, target validation, generated docs,
  fresh Astra review, R03/R09/R10, squash/push/PR remain outstanding.

- R08 heap-debug failure is now localized, not attributed to a speculative allocator bug.
  CLI build completed; compiling/running the preserved --heap-debug reproduction exits
  139 at the first loop FCC call (markers: start|zone|callable|before-first|).
  LLDB shows __rt_mixed_unbox dereferencing x0=0x3843bc40, the integer timestamp
  1999-11-30T12:00:00Z, not a heap pointer. The wrapper also used zero rather than
  current-time defaults. Artifacts remain at /tmp/elephc-datetime-binding.yhuLdd/
  mktime_defaults_heap{.php,.s, executable}. No process/build session remains live.
- Exact emitted wrapper evidence: mktime_defaults_heap.s around 133270-133365.
  _eir_main_callable_builtin_4951 loads nullable params, maps null tags to zero,
  calls _elephc_tz_mktime, and returns raw x0. The caller subsequently consumes the
  return as boxed Mixed. This is a callable-wrapper argument/return ABI mismatch.
- Source: codegen/lower_inst/runtime_wrappers.rs build_runtime_call_wrapper_function
  (around 160-230) creates a Builder and calls lower_registry_call directly. It
  does NOT invoke frontend lower_builtin_call_args, so the new Mktime argument
  strategy cannot fix this path alone. Wrapper signature and descriptor return
  representation must agree; do not patch heap frees to mask this bad pointer.
- Next action: unify defaults and checked timestamp representation across native
  callable wrappers and ordinary calls, then replay the strict heap-debug regression.
  A shared checked timelib bridge with explicit nullable mask and distinct success/
  timestamp fields is one candidate; first map raw-alias consumers and result-owner
  metadata. No native ABI change has been made yet. R08 and final docs/audit remain open.

- R08 latest source: registry-backed callables now select BuiltinArgumentLowering::Mktime
  from their home semantic descriptors. ir_lower/expr/mktime_args.rs binds/evaluates
  operands once, preserves nullable fields in owned hidden temps, takes one time()
  snapshot, completes defaults using AST expressions, and registers post-call cleanup.
  The AST declaration helper and EIR argument path share MKTIME_COMPONENT_FORMATS.
  Five optional catalog parameters of mktime/gmmktime are now Nullable(Int); hour
  remains required Int. Return metadata/checked ABI remains unresolved.
- Dynamic spread diagnostics now build messages via an AST exception-message helper,
  using the runtime count plus positional prefix and shared catalog bounds. Native
  checks pass for mktime missing/extra and strlen exact arity, including complete
  PHP messages. The obsolete min-guard wrapper was removed after a dead-code warning.
- 2026-09-07 resumed the interrupted test only after confirming its handle missing
  and no Cargo/rustc process live. Expanded local/UTC nullable and omitted-default
  fixtures cover direct, FCC, call_user_func, and computed call_user_func_array in
  AOT and opaque eval. The full focused mktime filter passed 16/16 in 130.91 seconds.
- New strict heap-debug test test_mktime_callable_defaults_release_temporaries FAILS:
  a 40-iteration FCC + nullable-array loop exits before `ok`, with stdout/stderr empty.
  The original assertion hid exit status; it now includes success/exit_code. No
  production memory fix has been guessed. Mixed array reads do copy fresh zval cells,
  and store_value_into_temp acquires ownership, so a double-free is not established.
- A retry rebuilt the cold task target (6m23s) and reproduced the empty-output failure.
  HEAD is still c423b1d56dc2a3cba712688b0fde6540aeeb4b3d; Cargo.toml/lock are clean.
  CLI build session 17079 is currently live. Preserved diagnostic fixture:
  /tmp/elephc-datetime-binding.yhuLdd/mktime_defaults_heap.php (start/zone/callable/
  loop markers). Once the CLI build completes, compile it with --heap-debug and
  inspect with LLDB; the test helper otherwise deletes its binary even on a signal.
- Docs workflow must be rerun after this new signature/semantic metadata change.
  Both new mktime.rs and mktime_args.rs sources are untracked: include them in the
  final source lock (stage or explicitly hash untracked sources) before another audit.

- R08 callback progress: removed the raw mktime-name substitution in name_resolver
  and the six cloned array reads in callable_probes. A known builtin with a computed
  call_user_func_array argument container now uses the ordinary AOT spread planner.
  Regression first reproduced six `args|` outputs; after the shared-path fix it
  prints once. No eval fallback was introduced for statically known builtins.
- The extended callback regression exposed Array(Void) reads producing Void SSA
  operands. Empty-array reads now materialize Mixed null, with corresponding
  AArch64/x86_64 result validation and null fallback. It also exposed fatal-only
  minimum-arity guards and missing maximum checks for builtin spreads. Shared
  spread lowering now throws ArgumentCountError for these bounds (legacy overloads
  retain TypeError). Expanded native test passes: mktime argument factory once,
  FCC + call_user_func fixed arguments, strlen through a computed array, and empty/
  extra mktime argument arrays caught by ArgumentCountError.
- R08 CORRECTION TO EARLIER EVIDENCE: frozen PHP reflection says hour is required
  int; ONLY the other five fields are nullable. Weak null hour is not a request for
  current hour. Corrected the AST helper, eval default loop, and frozen-clock test
  to preserve the supplied hour. Replaced the prior invalid `+12:00`/`+05:30` default
  timezone test fixture with the real identifier Asia/Kolkata and minute:null.
  The corrected native timezone-side-effect test passed; frozen-clock unit passed.
  The old hour:null/fixed-offset fixture is NOT valid reference evidence.
- R08 REMAINS OPEN: first-class/static callable lowering still selects the raw
  mktime RuntimeFnId without the AST default helper. Shared catalog mktime/gmmktime
  optionals are Int with null defaults, but PHP reflection requires Nullable(Int);
  return contract is Int instead of PHP int|false. Do not merely restamp raw i64 ABI
  results as Mixed: abi.rs elephc_tz_mktime/gmmktime return i64 and map None to -1,
  while PHP has valid timestamp -1. Reconcile representation and all callback forms.
  Current spread guard messages are catchable but generic/incomplete: exact PHP
  function/count wording (including actual supplied count) is still required.
- Assembly comment checker reports 45 pre-existing multiline instruction-comment
  findings in arrays.rs. The full base-to-worktree diff was inspected: new date
  changes there add ABI helper calls, not raw instruction/comment lines. Do not
  report that file's whole-file comment check green. Diff hygiene passes.
- AST snapshot replay passed after the required-hour correction. Both dynamic-new
  too-few-argument regressions also pass after the shared spread guard changes,
  including the builtin-specific diagnostic wording case. No live test sessions
  remain. A subsequent metadata/docs refresh and final source audit remain required.
  No commit, squash, push or PR publication yet.

- M01 completed: warning-free gen_builtins build with curl, registry + 1869-page
  regeneration, module-section refresh, and PHP comparison regeneration. Both
  documentation audits pass (0 errors; site validator checked 1890 generated pages).
  Enforced builtin/EIR architecture inventory passes with zero structural errors,
  and git diff --check is clean. getrandmax is attributed to Random in the source
  catalog and generated comparison. DateTime's overall status remains Partial;
  symbol-count coverage is not semantic compliance. No test/tool session remains live.

- R08 implementation in progress: name_resolver now calls a private DateTime AST
  helper with six nullable scalar parameters; one time() sample in the body feeds
  every date/gmdate default. Both generated declaration variants and the audited
  model include the shared AST builder in datetime/mktime.rs. The eval path uses a
  FnOnce clock and one timelib broken-down tuple, with no repeated current-time calls.
- Green: mktime_defaults_use_one_clock_sample checks both local/UTC modes, three
  timezones, three timestamps around epoch/year boundaries, and both omitted and
  explicitly null defaults. git diff --check passes. AST and native regressions
  are still pending. Native regression changes timezone in the last supplied named
  argument, to distinguish pre-argument and post-argument default sampling.
- Libtest compilation exposed a stale parity_tests.rs reference to singular
  cli_ini_set_decl. The date-timezone-aware implementation returns both declarations
  via cli_ini_set_decls; the parity inventory now includes that complete vector.
  The prior libtest run terminated with compile failure; corrected replay is live.
- The update-builtin-docs skill was reread; exporter regeneration/audits are required
  after the focused tests, including the already-fixed M01 Random attribution.
- R08 AST snapshot test now passes. Complete timelib/fallback model comparison
  also passes after fixing R01's two generated negative literals to e_neg(e_int())
  (the exact parser model), without changing their numeric meaning.
- IMPORTANT: R08 is not closed. Found callback bypasses while checking the other
  invocation surfaces: name_resolver/mod.rs:102 rewrites call_user_func_array mktime
  names to __elephc_*_raw; ir_lower/expr/callable_probes.rs:103 then clones arg_array
  six times into indexed reads, bypassing nullable defaults and potentially repeating
  side effects. callable_resolution.rs:51 lowers static Builtin bindings straight to
  runtime semantics as well. These need the same default/snapshot implementation,
  not a regression to eval for statically known calls. Add first-class, variable,
  call_user_func and call_user_func_array regressions before checking R08 complete.
- The focused native `mktime` filter passed all 14 tests under the dedicated TMPDIR,
  including the new supplied-argument timezone-change regression in AOT and opaque
  eval. No callback-bypass source edit has been made yet. Use `-p elephc`
  with future --lib commands: workspace default members otherwise rebuild other
  library test binaries even when their test filter matches zero tests.

- R04: removed the hand-maintained alias parameter/default tables. Aliases and
  call-array dispatch now use the shared builtin binder, which validates internal
  arity (including catalog min/max overrides), reports catchable ArgumentCountError
  for missing/extra values, and Error for unknown/overwritten names. Supplied
  argument evaluation still happens once before binding. Metadata class constants
  have a typed default descriptor and resolve through the native constant bridge;
  no PHP source is parsed for defaults. This also fixes the previously unsupported
  DateTimeZone::ALL gap when timezone_identifiers_list supplies only countryCode.
- Green: all 59 date/calendar aliases checked for zero/extra arity and materializable
  default descriptors in date_alias_shared_binding_validates_all_arities. Native
  test_datetime_opaque_eval_shared_argument_binding validates reordered strptime,
  idate and date_format names, skipped defaults, exact catchable error messages,
  and call_user_func_array errors. Named-argument regression filter: 33 passed
  again after the final class-constant and min/max arity changes. No running tests
  remain at this checkpoint.
- One native replay failed before execution because its runtime object disappeared
  from the shared OS temporary directory. A terminal failed run was confirmed and
  rerun with TMPDIR=/tmp/elephc-datetime-binding.yhuLdd; the native test passed.
  Preserve that task-local temp root for subsequent focused tests. Diff hygiene
  passes. R03 receiver/type-coercion/override behavior remains a separate open item.
- R08 next: eval_mktime_result_with_defaults samples each missing field separately
  (time/mktime.rs:49); name_resolver/expressions.rs inserts date() defaults among
  supplied arguments. Replace the AOT rewrite with a typed AST helper whose nullable
  parameters are evaluated before entry, sample time once in its body, and pass that
  timestamp to every default conversion. Eval needs the equivalent single snapshot.
  Update the stale libc wording on eval_mktime_result when touching it.

- R02: opaque-eval aliases now call the native AST procedural wrappers for
  timezone_open, date_modify, and date_interval_create_from_date_string, passing
  the hidden diagnostic line argument without broadly catching user exceptions.
  The first regression exposed missing wrapper bodies in opaque-eval reachability;
  all three are now retained by src/ir_lower/builtin_datetime.rs.
- Green: test_datetime_opaque_eval_procedural_failures_differ_from_methods checks
  three false results, function-specific warnings, continued execution, and the
  corresponding three throwing OOP forms. Single focused native test passed;
  git diff --check passed. No target-matrix or final closure claim.
- R03 receiver/override semantics and R04 input binding/coercion remain open.
  R04 investigation: registry/binding.rs already exposes shared parameter names,
  shape and defaults, but its positional fast path skips arity validation and its
  named-error paths return bare RuntimeFatal. Reuse the contract without retaining
  aliases.rs's manual parameter/default tables; preserve catchable PHP errors.
  interpreter/throwables.rs owns catchable Error/TypeError/ValueError construction;
  no ArgumentCountError helper currently exists. No R04 source edit yet.

- R01: inspected frozen php_date.c:845-978 and ran the oracle. The AOT helper
  also lacked valid j/o tokens and C-int semantics, so both engines were fixed.
  idate now validates one byte and the 23-token inventory, uses signed year %100
  for y, narrows results to signed 32-bit (all supported C ABIs), and maps the
  native -1 sentinel to false plus its warning. No development profile restored.
- Updated both generated AST variants and the cfg(test) PHP reference body.
  The new AOT/opaque-eval regression covers missing j/o, negative years, wrapped
  timestamps, PHP_INT_MAX, and invalid formats. All six test_idate-filtered native
  regressions pass. Argument normalization/coercion remains part of open R04.
  Source: https://github.com/php/php-src/blob/47b563cbb856ec19155aacc3246931dfacbebd21/ext/date/php_date.c

- R06: RuntimeValueOps has a byte-preserving notice path, with AArch64/x86_64 C
  wrappers checking E_NOTICE and shared suppression. AOT invalid timezone notices
  now gate the entire message on E_NOTICE and no longer filter the identifier
  fragment as E_WARNING. Invalid UTF-8 identifier bytes are not replaced.
- The opaque-eval regression exposed missing `@` parsing. Added a token and unary
  EvalIR operator plus exception-safe mask scopes. Native scopes read the raw mask
  so nested AOT suppression cannot overwrite the original mask; END_SILENCE keeps
  explicit nonfatal mask changes, matching the observed PHP behavior.
- Green: byte-preservation unit; three parser/interpreter suppression tests covering
  precedence, missing operand, normal/exceptional restoration and explicit mask
  changes; native test_timezone_invalid_setter_notice_masks_and_opaque_eval.
  Error-handler callbacks and broader unsupported assignment-expression forms were
  not certified by these tests; final surface/target review remains required.
- R05: eval_request_timezone queries DateDefaultTimezoneGet on every local-time
  operation when native hooks exist, releases its temporary cell, and falls back
  to context state only for standalone eval. date, strtotime, mktime, getdate and
  localtime use it. The native callback regression passes in both directions,
  including a timezone change during the same opaque eval fragment.
- R07: removed libc localtime_r, process TZ mutation, and the eval timezone mutex
  from date.rs. Broken-down fields now come from one vendored-timelib formatting
  snapshot and use i64 throughout, including year minus 1900. The opaque native
  regression passes for 10^17, PHP_INT_MIN and PHP_INT_MAX with oracle outputs
  3168875820, -292277022657 and 292277026596 for getdate year. R05 and R06 native
  regressions were replayed successfully after this change.
- Assembly comment checks for both eval-bridge output files and builtins/system.rs
  pass; git diff --check passes. No active exec/test sessions at this checkpoint.
- R01-R04, R08-R10, M01 generated-doc validation, and final review/matrix/squash/push
  remain open. No further reviewer was launched during this implementation batch.

- Rebase completed, compiler/exporter builds with curl, generated-doc structural
  and site checks pass. PHP comparison identified M01 independently.
- M01 source attribution changed from Standard to Random after the audit completed;
  exporter rebuild, regeneration and comparison rerun are pending.
- Full audit report has been read. Remediation is now authorized; no review agent
  remains running and no build/test session is live at this checkpoint.
- R06 next implementation surface: RuntimeValueOps only has warning/deprecated
  (runtime_ops.rs:500); add a typed notice path preserving original bytes and
  E_NOTICE masking. Native providers are runtime_hooks/ops/lifecycle_scalars.rs
  and fake providers interpreter/tests/support/runtime_ops/lifecycle_scalars.rs.
  Current date_default_timezone_set.rs uses warning and lossy UTF-8 conversion.
  Trace __elephc_eval_warning/__elephc_eval_deprecated wrappers before adding the
  notice ABI sibling on both architectures; do not merely relabel warning text.
- R09 must test both declared Mixed-to-Object returns (independent payload owner)
  and Object-typed ABI parameter passthrough (potentially borrowed payload).
  Merely comparing pointers or unconditionally retaining is insufficient.
