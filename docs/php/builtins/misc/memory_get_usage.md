---
title: "memory_get_usage()"
description: "Returns the amount of memory allocated to PHP."
sidebar:
  order: 639
---

## memory_get_usage()

```php
function memory_get_usage(bool $real_usage = false): int
```

Returns the amount of memory allocated to PHP.

**Parameters**:
- `$real_usage` (`bool`), default `false`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `memory_get_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_get_usage.md).
