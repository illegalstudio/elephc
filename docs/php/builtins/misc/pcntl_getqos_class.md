---
title: "pcntl_getqos_class()"
description: "Returns the current macOS thread quality-of-service class."
sidebar:
  order: 336
---

## pcntl_getqos_class()

```php
function pcntl_getqos_class(): mixed
```

Returns the current macOS thread quality-of-service class.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getqos_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getqos_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getqos_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getqos_class.md).
