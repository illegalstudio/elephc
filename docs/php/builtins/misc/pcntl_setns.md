---
title: "pcntl_setns()"
description: "Joins one Linux namespace of the selected process."
sidebar:
  order: 338
---

## pcntl_setns()

```php
function pcntl_setns(int $process_id = null, int $nstype = 1073741824): bool
```

Joins one Linux namespace of the selected process.

**Parameters**:
- `$process_id` (`int`), default `null`, optional
- `$nstype` (`int`), default `1073741824`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setns.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setns` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setns.md).
