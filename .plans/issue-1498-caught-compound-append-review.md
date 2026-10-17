# PR #1700: caught compound append operations

Reviewed head: `5d6b9b41fd2c41973ae50609eedf72508937787c`.

The reviewed compiler evaluates a compound append before creating its slot. A caught
DivisionByZeroError therefore leaves an empty array where PHP preserves one null element.
The fix evaluates the eager RHS first, reserves the slot, then applies the operation.

Nested updates capture the ordinary stored Mixed cells before key diagnostics can invoke
user code. Compiler-only generated RefAssign markers describe these leases without setting
PHP reference-entry flags. Their EIR slots own the actual boxes, remain visible to exception
cleanup, and are retired in reverse order. Stores replace the selected box's payload instead
of replaying a path into a parent the callback may have replaced.

Explicit keys are normalized once through the ordinary array writer. Generated helper calls
have distinct synthetic spans so checked-call result caches cannot confuse their identities.
Intermediate stores preserve sparse keys; leaf reads keep missing-key diagnostics and restore
null after the existing writable-fetch autovivification, before the operation runs.

Two related ownership paths are necessary for the callback cases: array reads pin an owning
receiver over diagnostic callbacks, and nested assignments release every owned RHS temporary
after the setter retains/boxes its value (including fresh arrays, rather than strings alone).
String-offset branches share this final cleanup to avoid releasing an RHS twice.

Validation on the final implementation:
- Warning-free `cargo build` and `git diff --check`.
- 102 focused append codegen tests; 7 append parser tests.
- 21 COW/cycle controls, 19 checked-operand regressions, 1 indexed-read lifetime control.
- All 3 new regression tests also pass with stack register allocation.
- 20 PHP fixtures, IR optimization on/off, executed on macOS ARM64, Linux x86_64 and
  Linux ARM64: 120 complete-stdout/warning-count comparisons, all heap-clean.
- Direct, nested and reentrant-handler fixtures with optimization on/off assemble as
  static libraries for iOS device and Simulator: 24 program/runtime objects; program runtime
  references all have emitted definitions. iOS binaries were not executed.

All local package entrypoints were forced fresh before freezing the baseline, avoiding shared
Cargo target artifacts from another worktree. Earlier prototypes and invalid runs remain in
`/private/tmp/elephc-1700-review`; their evidence does not substitute for the final checks above.
The full test suite and complete CI matrix were not run locally.
