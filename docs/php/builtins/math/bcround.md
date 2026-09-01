---
title: "bcround()"
description: "Rounds an arbitrary-precision decimal number."
sidebar:
  order: 278
---

## bcround()

```php
function bcround(string $num, int $precision = 0, int $mode = 1): string
```

Rounds an arbitrary-precision decimal number.

**Parameters**:
- `$num` (`string`)
- `$precision` (`int`), default `0`, optional
- `$mode` (`int`), default `1`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcround.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcround.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcround` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcround.md).
