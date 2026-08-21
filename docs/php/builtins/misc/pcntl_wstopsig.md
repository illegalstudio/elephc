---
title: "pcntl_wstopsig()"
description: "Returns the stopping signal encoded in a child wait status."
sidebar:
  order: 357
---

## pcntl_wstopsig()

```php
function pcntl_wstopsig(int $status): mixed
```

Returns the stopping signal encoded in a child wait status.

**Parameters**:
- `$status` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wstopsig.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wstopsig.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wstopsig` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wstopsig.md).
