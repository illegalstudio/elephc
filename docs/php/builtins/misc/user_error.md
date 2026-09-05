---
title: "user_error()"
description: "Alias of trigger_error."
sidebar:
  order: 358
---

## user_error()

```php
function user_error(string $message, int $error_level = 1024): bool
```

Alias of trigger_error.

**Parameters**:
- `$message` (`string`)
- `$error_level` (`int`), default `1024`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/user_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/user_error.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `user_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/user_error.md).
