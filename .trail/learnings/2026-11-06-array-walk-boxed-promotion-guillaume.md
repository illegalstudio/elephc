# Restored array_walk reference/key promotion and first-class/callable-array callbacks

- The base only gave local arrays PHP's writable walk semantics when the storage was already a
  boxed PHP array. A plain `array<int>` local silently lost by-reference element writes, and a
  two-parameter callback failed at run time because the unboxed eight-byte wrapper passes only
  the value. `check_array_walk_callback_builtin_call` now detects callbacks that declare the
  value by reference, read the key, accept a variadic tail, or resolve at run time, replaces the
  contextual value type with writable `Mixed`, and records the array argument as a boxed
  reference output. The existing by-ref argument promotion lowers the local to a rebindable
  Mixed cell and routes the walk through `__rt_array_walk_boxed`.
- `__rt_array_walk_boxed` now accepts `CALLABLE_DESC_KIND_FIRST_CLASS` and
  `CALLABLE_DESC_KIND_ARRAY`, so first-class method/function descriptors and callable arrays
  reach the uniform `(value-ref, key)` invoker instead of raising the opaque-callback error.
- The checker now supplies a writable Mixed dummy for the boxed value parameter, so a direct
  `bump(...)` callback whose first parameter is by-reference passes contextual validation.
- Verified against PHP 8.5 on aarch64 and linux-x86_64/qemu: local by-ref closure, keyed assoc
  walk, string function name, first-class global/instance/static callbacks, instance/static
  callable arrays, by-ref capture, and `array_walk_recursive` with `(value-ref, key)`.
- New regression: `tests/codegen/arrays/callbacks.rs::test_array_walk_local_reference_callbacks_promote_to_boxed_storage`.
  Focused: `array_walk` 19 passed, `arrays::callbacks` 133 passed, boxed walk 9 passed,
  `boxed` 359 passed.
- A one-parameter by-value closure still emits `runtime.array_walk` (the concrete fast path), so
  the promotion is limited to callbacks that need PHP's writable/key contract.
