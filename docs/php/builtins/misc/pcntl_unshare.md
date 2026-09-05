---
title: "pcntl_unshare()"
description: "Disassociates selected Linux process execution contexts."
sidebar:
  order: 349
---

## pcntl_unshare()

```php
function pcntl_unshare(int $flags): bool
```

Disassociates selected Linux process execution contexts.

**Parameters**:
- `$flags` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_unshare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_unshare.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_unshare` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_unshare.md).
