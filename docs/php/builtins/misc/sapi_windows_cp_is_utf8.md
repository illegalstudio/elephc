---
title: "sapi_windows_cp_is_utf8()"
description: "Reports whether the active Windows code page is UTF-8 compatible."
sidebar:
  order: 694
---

## sapi_windows_cp_is_utf8()

```php
function sapi_windows_cp_is_utf8(): bool
```

Reports whether the active Windows code page is UTF-8 compatible.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sapi_windows_cp_is_utf8` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/sapi_windows_cp_is_utf8.md).
