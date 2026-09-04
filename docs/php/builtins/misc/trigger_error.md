---
title: "trigger_error()"
description: "Generates a user-level PHP error."
sidebar:
  order: 355
---

## trigger_error()

```php
function trigger_error(string $message, int $error_level = 1024): bool
```

Generates a user-level PHP error.

**Parameters**:
- `$message` (`string`)
- `$error_level` (`int`), default `1024`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/trigger_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/trigger_error.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `trigger_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/trigger_error.md).
