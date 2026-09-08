# DateTime Closure/object property storage parity

## 2026-09-06 focused checkpoint

### Latest continuation

#### Rebase complete; fresh Astra audit running

- Rebase completed at HEAD c423b1d56dc2a3cba712688b0fde6540aeeb4b3d.
  Base is 0499d54914dba2ee612b6307cf3355266994428d; rev-list base...HEAD = 0/11.
  The backup remains backup/datetime-before-main-20260906T172432Z (395fc62d8c).
- Main's shared symbol catalogs are retained. DATE_* strings and LC_* locale
  categories are migrated into the shared constant catalog; E_ALL comes from
  PhpVersion::all_error_levels(), used by AOT prescan, eval, and INI masks.
  Obsolete date/error/locale constant modules are removed, their data preserved
  in the catalog/typed helpers. Synthetic body_exact uses keep_unread_params;
  by-ref variadic transcription and Signature defaults preserve main's additions.
- Runtime profiles remain minor-selected, stable .0 spellings, empty suffix.
  OPcache now derives its spelling from the same PhpVersion, including 8.6.
- Shared contract tests passed 24/24 before the new locale unit test. The full
  exporter build `cargo build --example gen_builtins --features curl` now succeeds
  warning-free after post-rebase metadata/DefaultSpec integration fixes.
- Disk exhausted during an earlier archive build. Removed only generated cache
  artifacts under /tmp/elephc-datetime-target: one inactive incremental directory,
  then `CARGO_TARGET_DIR=... cargo clean -p elephc` (27.5 GiB logical artifacts).
  No source or Git backup removed; rebuild succeeded afterward, about 11 GiB free.
- Fresh agent /root/astra_datetime_post_rebase is GPT-6 Astra LOW, explicitly
  requested by the user. READ-ONLY source audit, report destination:
  /tmp/elephc-astra-datetime-post-rebase-audit.md. Do not edit audited source until
  its final report/snapshot lock. It is reviewing all prior A01-A14 and new code.
  Immutable source hash command:
  `/usr/bin/git diff 0499d54914dba2ee612b6307cf3355266994428d --binary -- src crates Cargo.toml Cargo.lock scripts/audit_builtin_eir_boundary.py | shasum -a 256`
  yields 178170104ae833f7d937a582b819998827304ddcbc1dee69195d9d649d5e9461.
  Spec hash remains 5849b7b54068a5db5b6b3ad999c20a45fa7c73804aaf21ef52e991fca8f96450.
  Use the immutable base SHA, not mutable origin/main, during verification.
- Docs regenerated with curl: 987 functions, 190 classes, 1089 constants; 1869
  pages rendered, module sections updated. audit_builtins and site compatibility
  pass (1890 pages checked); EIR boundary/target-architecture audit passes with
  zero structural errors. gen_php_comparison FAILS because getrandmax was
  assigned Standard but the PHP baseline assigns Random. Queued this exact
  source correction to Astra; apply after its final lock, then regenerate again.
- Auditor's provisional findings include active eval date aliases shadowing
  registry/native dispatch, idate/timezone_open/procedural method semantics,
  timezone-context/notice handling and libc date helpers, optional extra arguments
  in DateTime subclass overrides during interface dispatch, and true-alias
  Mixed-box/Object return cleanup. Await the complete report and distinguish
  borrowed Object-parameter ABI returns from owned Mixed-to-Object boundaries.
- All build/docs exec sessions are terminal; the review agent is the live work.
  Final audit remediation, rebased focused/matrix replay, final squash and push
  remain open. Generated documentation changes are deliberately outside the
  source hash and have not been committed during the review.

#### Rebase checkpoint and user profile correction (2026-09-06)

- The ISO identity regression now repeats factory/getter/destruction cycles and
  requires exactly ZERO live blocks (the former <=16 tolerance was removed).
  It passes. Other freshly green regressions: windowed/special transitions,
  virtual DST snapshots, truncated-unserialize fuzz, both restoration cases,
  dynamic serializer FCCs and concrete date-interface mutator FCCs.
- The last leaks were (1) implicit concrete-local-to-Mixed boxing hidden in
  backend LoadLocal, now explicit EIR MixedBox with cleanup, and (2) persisted
  string temporaries copied into associative literals without releasing the
  source. All changes preserved in local commit 395fc62d8c and backup branch
  backup/datetime-before-main-20260906T172432Z. Diagnostic PHP/binary/assembly
  files are now under /tmp/elephc-dateperiod-debug.Thzi9M, not in tests/.
- Fetched origin/main (0499d54914), then started rebase. It is paused at replay
  1/11, bc8a5f477f. Initially 492 conflicts; 475 generated documentation/registry
  conflicts were provisionally resolved to main and MUST be regenerated after
  the code merge, using update-builtin-docs (skill read in full; current command
  includes cargo build --example gen_builtins --features curl).
- Main moved constants/classes/PHP profiles into the shared builtin contract.
  Resolved names.rs/symbols.rs/php_version.rs to use main's shared sources;
  retained the date_reflection_signatures module in types/mod.rs. Migrated 14
  DATE_* strings into catalog_constants.rs (DATE_ISO8601_EXPANDED since Php82)
  and removed obsolete date_constants.rs. E_ALL's version-sensitive value is
  NOT yet migrated: error_constants.rs remains conflicted and needs transfer
  into the shared constant resolution paths, with AOT/eval agreement.
- USER OVERRIDE: supported versions are minor language profiles, not the frozen
  oracle's development version. Do NOT restore 80510/8.5.10-dev/4.5.10-dev in
  runtime support during later rebase commits. Shared PhpVersion now keeps
  80500, 8.5.0, 4.5.0 and an empty prerelease suffix; selectors remain 8.X.
  AOT/eval/OPcache and profile expectations were adjusted likewise. Only the
  rejection test retains the dev string. Isolated shared PhpVersion unit tests
  pass 4/4; no whole-project build is claimed during the conflicted rebase.
- Still 12 conflict paths at this checkpoint (before staging profile edits):
  catalog_data.rs, registry.rs, support.rs, Magician constant_eval.rs,
  codegen/frame.rs, mixed_narrowing.rs, prescan.rs, synthetic_class.rs,
  synthetic_class/transcribe.rs, datetime/gate.rs, checker/driver/init.rs,
  types/error_constants.rs. Frame and mixed_narrowing hunks are comment-only.
  Synthetic builders must preserve main's keep_unread_params and new variadic
  behavior while implementing body_exact via Signature.keep_unread_params;
  remove branch's redundant consume_unread_params fields. The extracted method
  transcriber must also preserve main's by-ref variadic support.
- No fresh Astra audit, squash or push yet. Finish rebase first, then validate
  the rebased candidate and request the single GPT-6 Astra low audit.

#### Subsequent fixes and current remaining failure

- Fixed `lower_date_magic_restore_properties` and the reference-filter path:
  retain their input and ensure a writable hash before consuming it. Previously
  assigning the returned borrowed pointer freed the caller's array. Both
  restoration regressions now pass (current snapshot and nullable/invalid bounds).
  A sampled timeout was a free-list loop during the caller's next hash mutation.
- First-class callables now infer runtime method signatures for Mixed/Union
  receivers; descriptor returns unbox owned single-pointer heap payloads (arrays,
  objects, callables, iterable) and release their Mixed wrapper. The dynamic
  DateTime serializer FCC regression passes across all four receiver forms.
- `DatePeriod` virtual DST snapshot expected getEndDate class was wrong. Full
  fixture replay through /tmp/php-src-oracle.zqYHAJ/sapi/cli/php (8.5.10-dev)
  confirms DateTime, the start class. Expected output corrected; fresh Elephc
  rerun of that exact fixture remains to be recorded.
- Fixed fixed-offset POSIX transition crash: UTC has posix_info but null DST
  rules. Added typed TimelibPosixInfo and paired C/Rust layout assertions; do not
  call timelib_get_transitions_for_year without both rules. New unit regression
  passes for UTC/Etc-GMT zones over MIN/zero/future through MAX, with PHP oracle
  agreement. `test_timezone_transitions_get` now passes. Windowed/special test
  still needs its fresh replay.
- Extended runtime alias comparisons in call-argument cleanup to raw Object
  pointer pairs (not raw-versus-boxed). The new DateTime temporary handle test
  passes, with PHP oracle output 1:1:2:1. This removes two retained ISO scratch
  object handles. The factory now releases start before interval, restoring the
  original LIFO pool order; both AST/reference builders were kept consistent.
- ISO identity output now matches 1,2,3,4,5:11:1, and a new recycled-handle
  regression passes with PHP oracle output 1:3:2:4. However the original heap-debug
  assertion now exposes 56 live blocks / 7480 bytes (threshold 16); it remains
  FAILED. Do not relax this threshold. Diagnose remaining allocation ownership.
- Current diagnostic: tests/dateperiod_heap_debug.php is an agent-created copy
  of that fixture; cargo run -- --heap-debug built the CLI successfully.
  /tmp/elephc_date_live.py supplies an LLDB date-live command to inspect the heap
  at __rt_heap_debug_report. Prior debug fixture/source/assembly were moved to
  /tmp/elephc-dateperiod-debug.Thzi9M, not discarded.
- LLDB inspection works with CLI --keep-symbols and breakpoint
  `_rt_heap_debug_report`; the script accepts `date-live`. The 56-block dump
  contains one live ParsedHandlePeriod (class id 22, payload 576), its handleless
  DateTimeImmutable and DateInterval backing objects, and three state hashes
  with timestamp/timezone_name/is_localtime/microsecond/civil_* keys.
- Fixed an additional ownership omission: ReturnBoundaryMixedToObject explicitly
  returns an owned object, but value_is_owning_temporary did not recognize the
  op. Adding it to that list reduces the ISO test to 38 live blocks / 4912 bytes.
  Identity remains correct; the memory assertion still FAILS. The current CLI
  diagnostic executable predates this latest one-line fix; rebuild before
  comparing a new live-block dump. All launched sessions are terminal.
- Next ownership lead (not yet a confirmed fix): boxed state arguments to
  DatePeriod::__elephc_rehydrate_datetime are conservatively retained because
  return-alias analysis returns Unknown for the internal constructorless
  allocation and invalidates variables passed to static calls without checking
  whether their parameters are by-reference. Do not blindly compare raw objects
  to Mixed box addresses or weaken memory assertions. Source locations:
  src/types/return_alias.rs expr_alias/apply_expr_effects,
  src/ir_lower/expr/nullable_method_calls.rs argument cleanup.
- Assembly comment checks for object_props.rs/callables.rs and diff check pass.
  Fresh Astra low audit, full relevant replay/matrix, rebase, squash and push are
  still outstanding; no new review approval is claimed.

- Concrete DateTime-family calls through DateTimeInterface now resolve across
  checker, EIR and backend without adding fictional reflected interface methods.
  `test_date_period_get_end_date_preserves_immutable_concrete_class` passed.
- Added `test_datetime_interface_concrete_mutators_and_callables`; it passes for
  mutable and immutable receivers and verifies Reflection still excludes modify.
  Descriptor invoker Object results now unbox with an owned payload reference
  and release their temporary Mixed box, using shared target-aware ABI helpers.
- Corrected the misleading dispatch docblock and assembly comment alignment.
  `git diff --check` and assembly checks for callables.rs/method_resolution.rs pass.
- Corrected DatePeriod's storage clone to call __elephc_clone_for_period_storage
  in both generated AST variants and the test-only reference builder.
  ISO identity regression improves from 1,4,5,7,8 to 1,4,5,6,7; expected remains
  1,2,3,4,5. Two handles retained during ISO construction are still unresolved.
  The factory constructs a DateTime plus DateTimeZone through
  __elephc_timelib_period_datetime and converts it via createFromInterface;
  investigate temporary/returned-alias ownership before assuming the leak source.
- Fresh restoration replay: clones_current_snapshot fails with Invalid
  serialization data for DatePeriod object; nullable_state_and_recurrence_bounds
  prints nullable:1:1| then its binary times out after 60 seconds. All launched
  test sessions have terminated. No fresh final Astra audit, squash or push yet.

- Fixed constructor-free `__rt_new_by_name` allocation on AArch64 and x86_64:
  classes with `_class_object_dynamic_prop_flags` now receive an owned Mixed
  dynamic-property hash before their property initializer runs. Previously the
  tail stayed null, and DateInterval restoration crashed in `__rt_hash_set`.
- `test_datetime_unserialize_truncated_object_fuzz_payloads_match_php_src` passes
  with the initial hash capacity aligned to direct allocation (4). Native macOS
  evidence only; the x86_64 implementation still needs executable validation.
- Assembly-comment alignment for `new_by_name.rs` and `git diff --check` pass.
- Next reproduced failure:
  `test_date_period_get_end_date_preserves_immutable_concrete_class` fails during
  checking with `Undefined method: DateTimeInterface::modify`. The nullable
  interface receiver reaches `infer_method_call_on_interface_type`, which only
  looks at declared interface methods. Preserve reflection signatures while
  resolving runtime concrete DateTime-family methods consistently in checker,
  EIR signature/result inference, and callable paths.
- Remaining focused failures, fresh Astra low audit, target validation, rebase,
  squash and push are still open. This checkpoint is not campaign closure.

- [ ] Separate declared PHP property type from physical runtime storage.
- [ ] Store declared `Closure` properties as callable descriptors and generic `object` properties as boxed `Mixed` cells.
- [ ] Preserve direct, reference, DateTime magic-hydration, GC, serialization, and debug ownership paths.
- [ ] Add focused php-src differential regressions only after the source-audit consensus gate.
- [ ] Re-run Kimi K3, GLM 5.3, and GPT-5.6 Sol on one frozen candidate.

## Why this is required

`DateTime::__unserialize()` delegates custom property restoration to ordinary property assignment.
PHP accepts a `Closure` for a declared `object` or `Closure` property. Elephc currently
boxes closures with runtime tag `10`, whose low word is a callable descriptor, not an object
layout pointer. Treating that pointer as `PhpType::Object` would send it to
`__rt_decref_object`, object class-id dispatch, serializers, and object debug walkers; that
leaks descriptor captures or reads descriptor metadata as an object header.

The source evidence is `src/codegen/lower_inst/core_closures.rs`,
`src/codegen_support/runtime/callables/descriptor_release.rs`, and
`src/codegen_support/runtime/arrays/decref_object.rs`. The php-src behavior follows
`ext/date/php_date.c` custom-property restoration through `update_property()` and normal
typed-property verification in `Zend/zend_execute.c`.

## Representation contract

The declaration type remains authoritative for checker diagnostics and reflection.

| Declared PHP type | Physical property storage | Required dynamic tags |
| --- | --- | --- |
| `Closure` | `PhpType::Callable` | `10` only |
| `object` | `PhpType::Mixed` | `6` object or `10` Closure |
| named class/interface | `PhpType::Object(name)` | `6`, compatible class/interface only |

`PropertySlot` must carry both declared and storage types. The corresponding runtime
storage mapper must be shared by EIR property reads, codegen slot resolution, and emitted
per-property runtime metadata. Do not convert a descriptor to an object pointer or create a
wrapper: both choices break Closure identity and handle ownership.

## Required implementation surfaces

1. Property lowering
   - `src/ir_lower/expr/property_access.rs`
   - `src/codegen/lower_inst/objects/property_resolution.rs`
   - property loads, stores, reference cells, and result materialization

2. Runtime metadata and lifecycle
   - `src/codegen_support/runtime/data/user.rs`
   - GC descriptors, serialization descriptors, var_dump/print_r/JSON rows, defaults, and
     `object_free_deep` tag selection
   - callable retain/release must always use descriptor helpers; Mixed slots must use
     `__rt_decref_mixed`

3. PHP-visible Closure handling
   - Mixed `is_object`, `get_class`, `get_debug_type`, `instanceof Closure`,
     `spl_object_id`, `spl_object_hash`, equality, and debug paths need tag-10 support
   - serialization of a Closure must throw `Exception: Serialization of 'Closure' is not allowed`
     (including nested property and array walks), never serialize it as `N;`

4. Tests after audit consensus
   - direct and reference DateTime-subclass `__unserialize()` assignments for `Closure`,
     `object`, named classes, and interfaces
   - capture ownership / release balance and object identity
   - native PHP differential fixtures plus target-aware focused codegen coverage

## Closure debug-record ABI

`Zend/zend_closures.c:602-704` exposes an ordered debug projection rather than an ordinary
object-property layout. The existing 64-byte descriptor header must remain unchanged:
`kind`, `entry`, `php_name`, `signature`, `environment`, `invocation`, and `invoker` keep
their offsets, while runtime captures still start at byte 64.

Extend the *invocation record*, after its existing seven words, with an optional pointer to
`CallableDebugRecordV1`; older invocation offsets stay valid. The V1 record must carry a
version, PHP closure/fake/user/static flags, primary name, source file, source line, and an
ordered binding table. Each binding records a provider (`capture`, `static local`, or hidden
`this`), name, runtime tag, by-reference bit, and role (`static`, `this`, `internal`).

The record is created explicitly from closure lowering and function metadata. Do not infer it
from descriptor kind or `php_name`: fake closures, nested closures, bound `$this`, and static
locals need their own provider identities. `this` is a hidden Mixed slot for every non-static
closure; top-level closures store null, methods store the receiver, and `Closure::bind` replaces
the same slot.

Add dedicated tag-10 Closure branches to var_dump and print_r runtime walkers. They use the
existing descriptor object handle, render the ordered `name|function`, `file`, `line`, `static`,
`this`, and `parameter` projection, and never send the descriptor to a class-id object walker.
The signature record already supplies parameter names, reference flags, required/optional state,
and variadic state. Projected captured/static values are borrowed, retained or boxed only for
their renderer, then released; the descriptor itself is never retained or released by a debug
walk.
