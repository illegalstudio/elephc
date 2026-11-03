---
title: "error_log()"
description: "Writes a message to the configured error log destination."
sidebar:
  order: 693
---

## error_log()

```php
function error_log(string $message, int $message_type = 0, string $destination = '', string $additional_headers = ''): bool
```

Writes a message to the configured error log destination.

**Parameters**:
- `$message` (`string`)
- `$message_type` (`int`), default `0`, optional
- `$destination` (`string`), default `''`, optional
- `$additional_headers` (`string`), default `''`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/error_log.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/error_log.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_log` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/error_log.md).
