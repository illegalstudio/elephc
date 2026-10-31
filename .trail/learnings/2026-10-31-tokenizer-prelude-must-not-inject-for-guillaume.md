---
title: Tokenizer prelude dynamic injection is load-bearing for stack traces
author: Guillaume
date: 2026-10-31
branch: feat/tokenizer-native-engine
touches:
  - src/tokenizer_prelude.rs
  - src/types/checker/clone_override_storage.rs
  - src/codegen/shared_mixed_string.rs
  - src/types/checker/functions/resolution/call.rs
---

`tokenizer_prelude::inject_if_used` treats `usage.dynamic_function_call` and `usage.constructs_dynamic_class` as tokenizer use. That looked over-broad, but it is LOAD-BEARING: the prelude's injected try/finally changes the runtime's uncaught-error path, and PHP-compatible stack traces depend on it. Removing the two dynamic flags made `test_buffer_object_loop_inlined_null_receiver_preserves_error` print only the fatal line, with no `Stack trace:` block, even though the fixture never touches the tokenizer. Keep the dynamic injection; do not narrow it to named tokenizer symbols.

The memory guard must not use a runtime string callable. `$writer = 'ini_set'; $writer('memory_limit', '-1')` made the checker emit a full callable invoker for the prelude (`_eir__eir___elephc_tokenizer_memory_limit_callable_builtin`), which pulled in the numeric conversion ladder (`_pow`), disabled the try-inlining shape the dead-code tests assert, and grew a trivial 30-line fixture's assembly from ~14k to ~156k lines. `call_user_func('ini_set', ...)` and a direct call both fail in the internal prelude unless `ini_set` is declared, and `__elephc_shared_ini` boxes its result through the mbstring runtime.

Final design: `tokenizer_declarations` inspects the program for an `ini_set` declaration (the opcache prelude injects one) and chooses the memory body at prelude-parse time. With `ini_set` present it calls it directly and restores the old value; without it the body returns the compiled heap capacity. No runtime callable, no mbstring box, and the memory-limit fatal stays exact when `ini_set` exists.

Other fixes this exposed:

- `record_runtime_callable_clone_override_destination` must skip the global widening when the first argument is syntactically a scalar/array (clone needs an object) or when no later argument can hold an array (only arrays carry `withProperties`). Otherwise a two-string runtime call reserves a property hash on every class and shifts throwable layouts by 8 bytes.
- `_eir_shared_mixed_to_string` raises a PHP Error and preserves the throwable frame on its failure path, so `emit_shared_mixed_string_helpers` must call `shared.require_mixed_string_runtime()` and report `handler_state`/`object_clone`. The test base runtime is a superset of those two families so fixtures that link the cached object still resolve them.
- Prelude arity checks only cover plain positional calls. A spread, named argument, or variadic prelude declaration changes the effective count after the call planner expands it, or deliberately accepts extras so its body raises PHP's runtime `ArgumentCountError` (`random_bytes(1, 2)`).

Verified: throwable construction-line, constrained-rethrow dead-code, buffer-loop both modes, try-inlining 15/15, random_bytes arity/named/unpack, 3786 bin tests, 2020 error tests, 19 tokenizer codegen tests, 54/54 vendored phpt.
