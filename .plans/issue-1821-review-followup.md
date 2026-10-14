# PR #1838 review follow-up

- [x] Complete forced reproduction on the reviewed source.
- [x] Prepare parser fallback to shared non-local expression stabilization.
- [x] Prepare adopted scalar cleanup for both runtime-name dispatchers.
- [x] Add constructor/destructor and numeric-source ownership controls.
- [x] Validate parser, source order, exceptions and supported targets on final source.
- [x] Prepare verified publication on the existing branch.

Reviewed head: `d8c9f9d4d1c4efe1b42710ad33915bdc97165e5c`.

The statement parser built a DynamicPropertyAccess Assignment with neither prelude nor
result target. EIR consequently lowered its value before invoking the non-local writer,
which lowered the same source again. Falling back to the common expression parser reuses
its receiver/name/RHS stabilization, including compound and lazy null-coalescing writes.
The old ad-hoc Assignment constructor is removed.

Both runtime-name slot dispatchers now match string, int, float and bool for after-store
adopted-box retirement, matching the static-name scalar policy. Borrowed/scoped owners
remain guarded by the shared ownership helper. Publication precedes release so a
Stringable destructor can observe the updated field.

Early unforced builds reused a shared Cargo target across worktrees and produced
inconsistent baselines (including a numeric pass). Those runs and manual probes are
retained but are not exact-head evidence. A forced reviewed-source rebuild is in progress;
final production files are temporarily saved in /private/tmp/elephc-1838-review/final-source.

The forced reviewed-source build confirms both findings: six constructors/destructors for
three assignments (clean heap), and 240 live Mixed cells / 9,600 bytes for 240 scalar writes.
The baseline compiler and manifest are retained. All production sources matched reviewed
HEAD for that build. Final source was then restored and the build script/library/executable
entry points touched to force the corresponding final rebuild.

Follow-up gate before leaving this campaign: check the published #1823 source with this
Stringable control after a forced worktree switch. Early unforced artifacts from the
shared target cannot be used to establish or dismiss a regression there.

Forced final-source validation:
- Fifteen focused property-ownership tests pass, including constructor/destructor counts,
  borrowed values, store visibility from destructors, numeric slots and lazy/source-order writes.
- Forty-five parser assignment tests pass, including the new statement normalization regression.
- Seven fixtures execute on macOS ARM64, Linux x86_64 and Linux ARM64 with explicit CLI
  optimization on/off: 42 PHP-exact, heap-clean native executions. These are real CLI switches,
  independent of any test-harness interpretation of environment flags.
- Runtime and source-order program assembly passes for macOS ARM64, iOS device and iOS Simulator:
  nine objects, without claiming iOS execution.
- The final forced test build and `cargo build --lib` are warning-free; `git diff --check` passes.
