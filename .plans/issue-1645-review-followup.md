# PR #1662 review follow-up

- [x] Reproduce the ownership and Closure-comparison review findings.
- [x] Correct ARM64 and x86_64 runtime and source-hash ownership.
- [x] Add and run focused regressions with EIR optimization on and off.
- [x] Replay PHP differential fixtures on both executable Linux targets.
- [x] Emit and assemble the Apple target fixtures and runtimes.
- [x] Regenerate and audit builtin documentation.
- [x] Confirm warning-free build.
- [x] Prepare the verified review response and publication evidence.

Reviewed base: `921208d338ee52b9e5f1e972107ece2c35229602` on
`Guikingone/elephc:fix/1645-array-diff-keys`.

## Changes

- Retain callable payloads copied into a fresh value/associative set result.
- Pass retained x86_64 payloads and temporary Mixed boxes in the runtime's `rax`
  argument register rather than the C ABI's `rdi`.
- Reject Closure string comparisons with a catchable PHP `Error`, preserving the
  left-to-right `__toString()` effects before a right-side Closure is rejected.
- Release partial results, completed comparison temporaries, and converted input
  hashes on that error path. Carry converted-input ownership as a hidden fourth
  runtime argument; borrowed inputs remain owned by their callers.
- Model comparison effects conservatively so an unused result does not erase
  errors or user-defined string conversions.
- Retain borrowed callable descriptors when inserting them into heterogeneous
  source hashes, on both architectures. LLDB refcount watchpoints identified this
  independent missing retain while testing repeated comparison errors.

## Evidence and limits

- Fourteen final PHP oracle programs exit successfully without warnings.
- Seven original review regressions pass locally, including repeated same/different
  Closure comparisons, owned literal survivors, nested arrays, conversion order,
  unused errors, and unused Stringable results. A separate eighth regression
  covers the source-hash callable retain.
- Initial debugging included an invalid prototype expecting `array_diff` to keep
  a Closure against an empty array. PHP rejects that conversion, and the final
  regression asserts the PHP Error instead.
- An initial native link failure came from a stale curl-enabled Magician flat
  archive in the shared target directory. Rebuilding the exact branch's default
  Magician archive repaired the environment; it was not a PHP behavior failure.
- Runtime-cache isolation did not repair the refcount defect. The missing source
  hash retain was verified through actual allocation/incref/release watchpoints.
- Final optimized/unoptimized, adjacent tests, target checks, generated builtin
  documentation audit, and publication evidence are recorded below as completed.

## Final focused validation

- `runtime_gc::value_set_owners`: 8/8 tests with EIR optimization on and 8/8 off;
  fourteen distinct fixtures per mode, all heap-clean. The off run also enabled
  the harness PHP cross-check; the standalone fourteen PHP oracles also passed.
- Adjacent `test_array_diff` and `test_array_intersect` filters: 6/6 and 5/5,
  with PHP cross-check enabled.
- Linux x86_64 and Linux ARM64: 56 actual static-binary executions total
  (14 fixtures x 2 optimization modes x 2 targets), all exit 0, exact PHP stdout,
  and `HEAP DEBUG: leak summary: clean`. The runtime assembly and program assembly
  came from this exact source; Docker's existing musl images supplied only the
  assembler, linker, libc and execution environment. No foreign bridge archives
  were used. Initial minimal runtime generation omitted clone/handler helpers
  needed by one callable wrapper; enabling those runtime features fixed the probe
  setup before the final replay.
- macOS ARM64 and both iOS ARM64 targets: 84 program emission/assembly pairs plus
  three runtime objects assemble successfully. iOS fixtures use static-library
  output as required; initial executable-mode probes correctly failed the CLI
  target policy. This is compile/assembler evidence, not iOS host execution.
- Builtin docs: exporter build with curl enabled; extraction/render, module
  sections, PHP comparison, builtin audit, site compatibility, and enforced EIR
  boundary audit all pass. Only the four affected effect descriptors changed.
- Assembly comment alignment and `git diff --check` pass.

Local evidence artifacts: `/private/tmp/elephc-1662-review` (ephemeral).
The complete CI matrix remains the publication gate after push.

- Final `cargo build -p elephc -p elephc-magician` passes without warnings.
