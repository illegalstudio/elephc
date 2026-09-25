---
title: "sapi_windows_cp_conv()"
description: "Converts a string between Windows code pages."
sidebar:
  order: 692
---

## sapi_windows_cp_conv()

```php
function sapi_windows_cp_conv(mixed $in_codepage, mixed $out_codepage, string $subject): ?string
```

Converts a string between Windows code pages.

**Parameters**:
- `$in_codepage` (`mixed`)
- `$out_codepage` (`mixed`)
- `$subject` (`string`)

**Returns**: `?string`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sapi_windows_cp_conv` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/sapi_windows_cp_conv.md).
