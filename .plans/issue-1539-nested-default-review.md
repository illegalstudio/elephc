# Nested lexical default review corrections

- [x] Reproduce all six review findings with native and opaque eval regressions.
- [x] Share recursive deferred-error detection between effect analysis and call cleanup.
- [x] Generate native EIR default helpers for method callback descriptors.
- [x] Preserve nested default evaluation and presence in native Reflection.
- [x] Transport nested native defaults safely as eval data with balanced temporary owners.
- [x] Normalize property defaults per node without making unused branches eager.
- [x] Validate both IR modes, all supported emitters, builtin documentation and focused tests.
- [ ] Publish signed fixes on PR #1696, verify the exact head and resolve the six threads.

The reviewed baseline is `d92077b94f6ad832fa49332077ac6d85a6258427`.
Main integration is kept separate from semantic corrections.

Deferred defaults must retain prefix constructor effects, lazy branch selection,
source default presence and source-order cleanup. Native EIR helpers may throw only
on the native call stack. Eval receives context-owned data, not callbacks that
unwind across Rust interpreter frames.

Validation covers 98 focused codegen regressions, 25 EIR/FFI/cache controls,
16 diagnostics and nine PHP oracle fixtures. Reflection uses persistent native
factory descriptors, not context-local eval closures or extra PHP object handles.
The complete CI matrix remains a post-publication gate.
