---
title: "bcadd()"
description: "Adds two arbitrary-precision decimal numbers."
sidebar:
  order: 293
---

## bcadd()

```php
function bcadd(string $num1, string $num2, int $scale = null): string
```

Adds two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcadd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcadd.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcadd` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcadd.md).
