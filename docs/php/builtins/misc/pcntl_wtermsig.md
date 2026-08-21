---
title: "pcntl_wtermsig()"
description: "Returns the terminating signal encoded in a child wait status."
sidebar:
  order: 358
---

## pcntl_wtermsig()

```php
function pcntl_wtermsig(int $status): mixed
```

Returns the terminating signal encoded in a child wait status.

**Parameters**:
- `$status` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wtermsig.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wtermsig.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wtermsig` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wtermsig.md).
