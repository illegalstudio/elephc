---
title: PHP FFI bridge implementation notes
author: Guillaume
date: 2026-11-02
branch: feat/ffi-php-src-compliance
cost: { minutes: 120, tokens: 251348 }
touches:
  - crates/elephc-ffi
  - src/ffi_prelude
  - src/codegen/lower_inst/objects/runtime_property_writes.rs
  - src/ir_lower/expr/property_access.rs
  - tests/codegen/ffi_php_src.rs
---

Key decisions and traps for the FFI bridge:
- Pointer-valued struct fields must return an independent CData copy (pointer_to), not a view of the field storage. PHP copies the pointer value: `$node = $root->next->next` must stay node3 after node1.next is rewritten. Array/offset pointer elements must remain views so `FFI::free($v[1])` zeroes the element.
- CData's synthetic `cdata` scalar field is lowered through a dedicated `__elephc_cdata()` method returning CData, because `__get(): mixed` boxes the value and a static CData result would misinterpret the box as an object pointer.
- `$ffi->new(...)` is a static method called through an instance; the static FFI::new cannot see the receiver context, so lowering maps it to `__elephc_instance_new`.
- Synthetic prelude builders do not model `self::$static[$key] = ...`; copy the static array to a local, mutate, and assign it back.
- For cdef global function pointers, `FFI::__get` must fall back to `function_cdata` when `global_kind` returns 0; `global_kind` sets a stale "Undefined C global" error that then leaks into `__elephc_throw`.
- libc `stdout`/`stdin`/`stderr` need dedicated storage symbols on macOS; dlsym(RTLD_DEFAULT) does not find them.

C-initiated callbacks (php-src 200 / bug79177):
- libffi closures in `crates/elephc-ffi/src/callback.rs` own a `Box<Cif>` and are never freed; C may retain the code pointer after PHP replaces the assignment.
- The PHP side must register three dispatchers via extern `callable` parameters: `(int $id, int $call): int`, `: float`, and `: void`. The extern callback trampoline is generated per callsite and updates its hidden descriptor slot; register each one once.
- Arguments cross as an opaque call handle; the PHP prelude reads them through `elephc_ffi_callback_arg_*`. `const char *` arrives as a PHP string, other pointers/aggregates as CData.
- A throw from the callback must be caught in PHP before it reaches Rust. php-src reports `Warning: Uncaught ...` plus `Fatal error: Throwing from FFI callbacks is not allowed`; `#1` names the innermost active FFI call, tracked by a thread-local stack in `call.rs`, not the callback global's name.
- The `zend_write` hook is a `RuntimeFeatures.ffi` variant of `__rt_stdout_write`: the runtime reads `elephc_zend_write_current` only in FFI programs, so non-FFI binaries never reference the bridge. `FFI::cdef` exposes that word as the `zend_write` global and initializes it to a Rust writer.
- `FFI::new()` is static and has no cdef context; use `$ffi->new()` for types declared in that cdef.
- Eval/backtrace programs add the compiler's hidden `mixed ...$__elephc_func_args` tail to EVERY function and closure, so extern callback validation and trampoline emission must strip `func_args::HIDDEN_ARGS_PARAM` before checking the C ABI. Otherwise every FFI prelude dispatcher is rejected as variadic in eval hosts.

Review fixes and traps (three-model review):
- libffi writes the whole declared return value through `ret`; a fixed 64-byte stack buffer corrupts the frame for larger structs. Use a stack buffer up to 64 bytes and a `calloc` buffer sized from the declared type otherwise.
- `release_cdata` must call `CData::release()` before dropping the box, and `CData::__destruct` must be private and clear the handle after release. Public destructors allow a manual second call and a double free.
- `elephc_ffi_field_type` / `offset_type` return the field/element type handle, while `field_pointer_type` / `offset_pointer_type` / `global_pointer_type` return the POINTEE handle. A helper that expects a pointee must not call `type_kind` expecting a pointer.
- The CData debug-info exception boundary must restore `_vd_seen` and print_r capture, then rethrow; the non-CData path must keep `GUARDED = 1` or `emit_guard_pop` is skipped (missing `_debuginfo_pad` / indent).
- `emit_print_r_capture_save`/restore labels must be parameterized when emitted more than once in one runtime function (guard, cdata, cdata_caught, done).
- `--with-ffi` needs `(with_crates.contains("ffi"), "ffi")` in `forced_groups`; otherwise the forced prelude is pruned again.
- libffi has no union type. Represent a union by its first field only when that field owns the union's full size/alignment; otherwise fail loudly. Expand arrays inside structs into repeated element types.
- Restrict static-method-through-instance to `FFI` (final): ordinary classes need runtime-class late static binding, and the static-type path silently picks the parent override.
- Ownership around CData reads: `offset_cdata`/`field_cdata` return pointer COPIES now, not views, so a returned `$buf[0]` survives the array. To keep `FFI::free($v[1])` zeroing the element, the copy carries `source` (the element address); `free_cdata` zeroes the source only while it still holds the copy's pointer value (the linked-list `del()` rewrites the source before freeing, so it must not be cleared). `cast`/`clone` of a non-pointer value copy into owned storage.
- `FFI::addr(<temporary>)` transfers ownership: the compiler lowers non-variable arguments to `FFI::__elephc_addr_temp` → `elephc_ffi_addr_owned`; the source loses `allocated/owned`, the pointer gains `owns_pointee`, and `release` frees the pointee then the pointer cell. `FFI::addr($variable)` keeps the shared reference.
- P2 arithmetic parity: php-src's `do_operation` only handles pointer/array operands. Scalar CData `+ - * / % ** & | ^ << >>` must raise `TypeError: Unsupported operand types: ...`. Keep `FFI\CData::__elephc_add` declared `CData` (its ABI must match the operator plan's `Object` result); `__elephc_sub` stays `mixed` because pointer difference returns `int`.
- `$cdata->cdata` is dynamically typed: scalar CData returns the PHP scalar, aggregate/pointer CData returns a CData view. Do not restore the old unconditional CData-returning accessor; `$x->cdata++` is handled by making property incdec use `AssignmentOperator::IncDec` (PHP string/number `++`), and `$x->cdata + 3` is now plain PHP arithmetic.
- `foreach` over a scalar C array yields PHP scalars; by-reference loops set a prelude flag (`__elephc_set_iter_by_ref`) so `current()` returns writable CData views. The EIR loop value is `Mixed` in both cases because `current(): mixed`.
- String assignment goes through `place::write_string`: `char` accepts exactly one byte, any `char[N]`/pointer raises php-src's `Incompatible types ... from PHP 'string'`, numeric fields use PHP's numeric-prefix conversion.
- `is_callable($cdata)` is lowered to `FFI::__elephc_is_callable` for statically typed CData and the bridge checks pointer-to-function plus the storage pointer; a null pointer field reads as PHP `null`.
- php-src fixture-only `zend_test` symbols live in `crates/elephc-ffi/src/php_src_fixtures.rs` behind the `php-src-fixtures` feature. `tests/codegen/support/runner.rs` always builds `elephc-ffi` with that feature; production `cargo build` does not export them.
- Property `++`/`--` on a string-valued property needs `AssignmentOperator::IncDec`; the old `Compound(Add)` coerced the string to a number. The delayed-static-receiver branch keeps the compound form. Regression: `tests/codegen/operators.rs::test_incdec_on_string_members_uses_string_semantics`.
- On this macOS host, compiled binaries can sit in `_dyld_start` for 20-40s under load (sample shows only `_dyld_start`); a 60s test-harness timeout can look like a code hang. Retry with a longer alarm before diagnosing a regression.
