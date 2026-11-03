---
title: "set_time_limit()"
description: "Limits the maximum execution time."
sidebar:
  order: 793
---

## set_time_limit()

```php
function set_time_limit(int $seconds): bool
```

Limits the maximum execution time.

**Parameters**:
- `$seconds` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/set_time_limit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/set_time_limit.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `set_time_limit` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/set_time_limit.md).
