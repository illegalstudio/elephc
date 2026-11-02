# Review scope: native/eval ownership repair

Review the supplied source diff and appended new modules/tests. This is a bounded
code review, not end-to-end Symfony acceptance and not proof of full PHP parity.
Do not infer access to files or test execution outside the supplied material.
Return concrete defects with code evidence, or state which invariants require
additional evidence. Do not invent runtime contracts. Prefer NO LOCK when the
provided evidence cannot establish a safety-critical ownership claim.
The first input line identifies the SHA-256 of the review bundle. Begin your
verdict with LOCK plus that hash, or NO LOCK plus that hash. Keep the response
under 1000 words; cite actual code paths for each finding.

## Intended invariants

1. Declared Throwable fields occupy the ordinary seven sixteen-byte property
   slots after the class id (120-byte payload). File is at 56, line at 72,
   previous at 104. Generic property access and GC use this declared layout.
2. `RuntimeOps::arg_array` owns temporary index cells, not its borrowed input
   values. The array setter borrows the key and retains the value.
3. Generated constructor/instance/static method readers own the Mixed result of
   array_get. It remains live through coercion, native execution and by-reference
   writeback, then is released on the common success/failure tail. Slots are
   zeroed before dispatch; each argument is read once per executed branch.
4. By-value string casts allocate a separate persisted native string, borrowed
   by the native callee. A second owner slot tracks it. Builtin Throwable message
   initialization transfers that conversion into a property and does not register
   it for argument cleanup. By-reference conversion ownership is unchanged.
5. `eval_const` returns a fresh cell for every literal. Materialized default
   arguments are fresh too. Their callers owe a release after invocation.
6. Property replacement retains the new owner and releases the old one. Heap
   properties publish the new value before decrementing the old owner so that
   a destructor observes the replacement. Strings use refcount-aware release.
7. An eval-unset entry does not consume the separate raw AOT-local owner. Missing
   local reload releases raw owned storage through the existing frame cleanup;
   it does not treat a definite or runtime-promoted reference pointer as raw.
8. An explicit literal key in an associative array is borrowed by insertion and
   reference binding; its temporary cell is released afterward.

## Evidence already obtained

- Seven Throwable layout/source/previous regressions pass, including runtime
  include getFile/getLine and repeated native owned-field updates.
- The opaque-eval repeated Exception reducer went from five leaked blocks per
  construction to zero after default, argument and string-property fixes.
- The three-int constructor reducer went from nine leaked cells per invocation
  to zero; native instance/static argument and return-alias regressions pass.
- The omitted string-default constructor reducer passes after conversion cleanup.
- Array, Mixed and string property replacement heap reducers pass. Native object
  property replacement executes the old object's destructor. Imported-owner unset
  regression now produces drd after missing-local cleanup.
- Three existing native/eval global replacement/transfer regressions pass.
- All changes preserve both AArch64 and x86_64 paths. The argument-owner emitter
  test and two Throwable layout tests pass across the five supported targets;
  no Linux executable validation has been run for this batch.

## Explicitly open / not acceptance claims

- No new full Symfony build since r154. It had HTTP 000 and native-heap corruption;
  the allocator/field-layout mismatch was proven with a hardware watchpoint.
- No general closure of eval loop/binary operand temporary ownership, numeric-key
  counter ownership, nested default-array ownership, reference-conversion ownership,
  all reference-unset semantics, Throwable traces, or full source provenance.
- Imported-owner destruction is currently tested at include return, not for an
  observable side effect after unset but before returning from that include.
- No full suite or complete target matrix result, no absolute reviewer consensus.

Focus review findings on regressions introduced by this repair and additional
necessary fixes in its ownership paths. A clean bounded review must not be worded
as a claim that Symfony works or that the entire compiler is compliant.

## Second snapshot: review response and newly exposed reference paths

GLM 5.2 returned NO LOCK on snapshot
9fdef6f81a08dc50c4a5d05b8836928c6a5daafd557da05db5f58b6816320f41.
Its report is supplied after the code. Assess its findings independently:

- Throwable message/previous replacement now publishes before releasing old.
- `__rt_decref_any` checks heap range before reading a header, so static source
  strings are accepted non-owning pointers. The helper source is supplied below.
- Array insertion borrows source cells; cleanup on a returned error still owes
  the temporary value/index. Preserving the first error is intentional.
- Native defaults are materialized into bound arguments in Rust BEFORE building
  the argument array. They reach the same generated array_get reader as explicit
  arguments. Separate raw-conversion ownership still needs its own review; do
  not conflate it with defaults bypassing that reader.
- `eval_const` allocates fresh cells; its implementation and the ownership
  classifier's consumers are provided for the literal-ownership contract.

Expanded by-reference tests exposed scope writeback that changed an existing
owned cell to Borrowed even when native code had only mutated it in place.
Variable writeback now preserves identical cells; a different cell is retained
for scope ownership. Four of five failing reference tests then passed.
The remaining Mixed constructor case passed after giving the mutable pointer
slot a separate owner AFTER argument preparation succeeds. Native assignment
may consume that owner; unchanged writeback releases it explicitly.

A further regression proved that blindly freeing a replacement Mixed cell could
destroy another variable's source cell. The latest code clones a shared
replacement before transferring its payload, dropping only the mutable slot's
reference to the shared source. Exclusive replacements retain the allocation-free
transfer. This last fix is still being validated. The original reader owner is
distinct from the mutable-slot owner and remains in the post-call cleanup bank.

## Third snapshot evidence

- Shared-source and unchanged Mixed-reference regressions both pass. The 18-test
  reference/exception/argument-preparation replay passes completely.
- Non-string defaults did expose retained raw CONSTRUCTOR conversions. The new
  record_constructor_conversion tracks array/object/iterable retains. Method
  conversions borrow those pointers and therefore do not record them. String
  conversions remain owned in both paths. The non-string default heap reducer
  and caller/property COW preservation test both pass.
- Complete classifier caller inventory: eval_call_arg_values; eval_assoc_array
  (literal values and keys); eval_spread_into_array; eval_foreach_owns_subject.
  Relevant complete implementations are supplied below. Constants are fresh
  cells, never aliases. Scalar spread/foreach subjects are invalid inputs, not
  hidden borrowed array constants (EvalConst has no array variant).
- _script_source_file is emitted as .ascii in user data, not managed heap storage;
  its exact definition is supplied, alongside the range-checking decref helper.
- The complete glibc AArch64 jmp_buf is 312 bytes, not the former 200-byte capacity
  after the handler header. TRY_HANDLER_SLOT_SIZE is now 336. All four PDO frames
  were migrated to size-derived offsets. Five-target emitter/layout tests pass.
  Real C probes in the existing Linux arm64 and x86_64 containers report sizes
  312 and 200, preserve the guard after the 336-byte handler and return by longjmp.
- The optimized compiler builds. Full Symfony r155 build is running; no HTTP
  acceptance result yet. Keep bounded patch review distinct from that goal.

Please evaluate the CURRENT hash, not the earlier review's source snapshot. An
older finding may have been fixed between hashes rather than misread originally.
