---
title: "pcntl_waitid()"
description: "Waits for a child state change and writes its signal information."
sidebar:
  order: 350
---

## pcntl_waitid()

```php
function pcntl_waitid(int $idtype = 0, int $id = null, mixed $info = [], int $flags = 4): bool
```

Waits for a child state change and writes its signal information.

**Parameters**:
- `$idtype` (`int`), default `0`, optional
- `$id` (`int`), default `null`, optional
- `$info` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `4`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_waitid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_waitid.md).
