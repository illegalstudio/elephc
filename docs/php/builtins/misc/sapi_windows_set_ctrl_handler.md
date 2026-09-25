---
title: "sapi_windows_set_ctrl_handler()"
description: "Installs or removes a Windows console control handler."
sidebar:
  order: 697
---

## sapi_windows_set_ctrl_handler()

```php
function sapi_windows_set_ctrl_handler(?callable $handler, bool $add = true): bool
```

Installs or removes a Windows console control handler.

**Parameters**:
- `$handler` (`?callable`)
- `$add` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sapi_windows_set_ctrl_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/sapi_windows_set_ctrl_handler.md).
