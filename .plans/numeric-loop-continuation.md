# Preserve numeric promotion around loop backedges

- [x] Reproduce the promoted-float mismatch on exact main in both IR modes.
- [x] Implement binding-scoped replay and closed numeric frame paths on both architectures.
- [x] Validate PHP, IR on/off, ownership, allocation scaling and all supported target emitters.
- [ ] Measure the final source identity against the same-platform native and PHP baselines.
- [ ] Complete independent review, publish the focused PR, and verify review/CI results.

Discovery baseline: main `51d65ce7cb98809459d0e1894f1e82d1f58cb341`.
Integrated base: main `bf6bd73334a2dd53c3faefc727c555a953845e03`.
Code Journal plan: `114c1388`; correctness task: `02bdf623`.

A local initialized as an integer can receive a checked arithmetic result that
promotes to float. A loop read lowered against its entry integer type can convert
that float back to integer on the next iteration. Discovery identifies narrow reads
in cyclic regions whose final slot is Mixed and has a numeric writer. Functions
with exception handlers widen conservatively because explicit CFG edges omit throws
into catch blocks. Replay uses ordinary Mixed reads from the first store, while
existing numeric closure proofs remain sealed per-call-site decisions.

Replay constraints identify both the PHP name and its storage binding ordinal.
`unset` followed by a new binding must not force an unrelated string into Mixed
storage, and parameter COW shadows retain their own storage ordinal. References,
global aliases and incoming scalar ABI representations keep their existing contracts.

The frame-cell proof separately verifies every numeric input origin and every local
writer. Unknown calls, references, nonnumeric values and escaping results retain the
runtime path. A selected Mixed add/subtract/multiply or numeric increment reads the
original payloads before updating its cell. Two integers use checked arithmetic;
overflow promotes from the original operands, and an existing float stays float.
Numeric relational comparisons retain exact signed integer precision and reject all
unordered NaN predicates. Strings and nonnumeric increments retain their helpers.
Both architectures preserve the same 24-byte Mixed layout and frame/ABI conventions.

Bounded raw-counter specialization may consume a numeric add/subtract/multiply
without changing its opcode. It still requires the complete existing integer-range
proof. Unknown inclusive bounds are not assumed to fit: numeric frame increments and
comparisons provide an allocation-bounded fallback instead.

Acceptance includes positive/negative overflow, fractional continuation, self-add,
multiply, by-value parameters, snapshots, reference fallbacks, exceptional backedges,
inclusive bounds, numeric comparison edges, and string rebinding on either side of
`unset`. Measurements must identify the exact validated source and reject noisy
comparisons. Execution evidence is local Linux x86_64; CI supplies the executable
Linux ARM64/macOS matrix and target-specific iOS validation.

Nonoverflowing increment recurrences retain their original scalar lowering after a
discarded IntegerRange trial on the fully widened graph. By-value observations use
authoritative parameter modes, while aliases, eval and exceptional CFGs fail closed.
Local-copy writer discovery follows ordinary assignment chains with cycle protection.


The closed integer/float origin analysis is shared with concat-reset elimination.
A numeric increment cannot allocate string scratch storage, and numeric retirement
cannot run object destructors. References, unknown writers and string domains retain
their ordinary effects and cleanup paths. Packed float field writes materialize a
Mixed source through the declared-property guard before publishing the payload;
those writes keep conservative exception effects until materialization.
