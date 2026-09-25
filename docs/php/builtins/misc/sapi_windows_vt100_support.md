---
title: "sapi_windows_vt100_support()"
description: "Queries or changes VT100 support for a Windows console stream."
sidebar:
  order: 698
---

## sapi_windows_vt100_support()

```php
function sapi_windows_vt100_support(mixed $stream, ?bool $enable = null): bool
```

Queries or changes VT100 support for a Windows console stream.

**Parameters**:
- `$stream` (`mixed`)
- `$enable` (`?bool`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sapi_windows_vt100_support` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/sapi_windows_vt100_support.md).
