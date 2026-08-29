---
title: "pcntl_get_last_error()"
description: "Returns the errno recorded by the most recent failing PCNTL operation."
sidebar:
  order: 333
---

## pcntl_get_last_error()

```php
function pcntl_get_last_error(): int
```

Returns the errno recorded by the most recent failing PCNTL operation.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_get_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_get_last_error.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_get_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_get_last_error.md).
