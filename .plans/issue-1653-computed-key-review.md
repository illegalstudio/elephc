# Issue #1653: computed key review follow-up

Reviewed PR #1666 at `3a2ca069e6ca29737b838f067dbd7a7278f541c9`.
Rebased all three original commits onto main `62cfeb85efb1d7bbcd149c528a8bc97d959fdbde`.
`git range-diff` reports all three patches unchanged. Publication uses a dedicated branch;
the original upstream PR branch is preserved.

## Review findings

- `PRRT_kwDORs6r_s6r1XNg`: `$a[$i + 0] += ($i = 1)` freezes the computed key
  before the RHS. Its bare-variable control uses the later index. Local, instance,
  static and assignment-expression forms now match PHP.
- `PRRT_kwDORs6r_s6r1XPL`: computed nested keys execute first, the value executes next,
  and the stable root/parent chain is fetched last. A reassigned root retains both
  its replacement contents and the final leaf write. Observers cannot see premature
  autovivification. Owned and borrowed keys are pinned across replacement and throws.
- The corrected effectful-key compound path reached an existing 40-byte Mixed numeric
  result leak. Concrete integer stores now retire the discarded source box after
  publication, for local/property/static receivers.

## Validation

- Warning-free `cargo build -p elephc` after rebase.
- Final standalone codegen harness, using this worktree's test support and freshly
  built library: 28 relevant tests (8 new, 18 existing write-order, 2 concrete nested
  GC tests), plus 17 support tests, all passed. This is not the full suite.
- Before rebase: focused `cargo test --lib propagate` passed 97 compiler unit tests
  and 4 matching Magician tests. The final rebased codegen replay is the integration
  evidence; the pre-rebase unit run is recorded separately.
- Ten PHP-oracle fixtures, EIR optimization on/off, on macOS ARM64, Linux x86_64 and
  Linux ARM64: 60 native executions, exact stdout, no warnings, zero live heap blocks.
- iOS device/Simulator: three fixtures, on/off, program and runtime assembled with
  their SDKs, 24 objects; all referenced runtime symbols resolve. No iOS execution.
- `git diff --check`; Rust module preambles present in every changed Rust file.
  No assembly emitter source changed. No builtin catalog/signature changed.

Reproducible suite filters:

```sh
cargo test -p elephc --test codegen_tests arrays::computed_write_order
cargo test -p elephc --test codegen_tests arrays::write_evaluation_order
cargo test -p elephc --test codegen_tests concrete_nested_receiver_reboxing
```

Local frozen reproducers, old/new libraries, complete heap stderr, native and iOS
results, and range-diff: `/private/tmp/elephc-1666-review/`.

## Trail check interpretation

Nine reported conflicts match keywords in this change's own new learning against
comments, docs, this validation record and regression witnesses. They are false positives: the implementation
captures only computed keys before the RHS, retains bare-variable keys for store time,
and fetches stable parent chains after the RHS. The computed/bare control and RHS
observer tests prove these distinctions. No immutable learning was edited.

Trail also reports overlap with the compound-append diagnostics, static-property
object-receiver and asymmetric-property branches. Those branches and other worktrees
were preserved; future merges must reconcile the shared parser/lowering modules.

## Publication boundary

The standing Trail rule requires pushes to the user's fork, while existing #1666 is
owned by the upstream repository. Its review threads must not be marked resolved
until the correction is published through the agreed route. Native target results
are not a claim that CI has run on the final published SHA.
