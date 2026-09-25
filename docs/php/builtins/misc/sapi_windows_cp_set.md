---
title: "sapi_windows_cp_set()"
description: "Sets the active Windows console code page."
sidebar:
  order: 695
---

## sapi_windows_cp_set()

```php
function sapi_windows_cp_set(int $codepage): bool
```

Sets the active Windows console code page.

**Parameters**:
- `$codepage` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sapi_windows_cp_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/sapi_windows_cp_set.md).
