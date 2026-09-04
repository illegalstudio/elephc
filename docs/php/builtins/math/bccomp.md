---
title: "bccomp()"
description: "Compares two arbitrary-precision decimal numbers."
sidebar:
  order: 295
---

## bccomp()

```php
function bccomp(string $num1, string $num2, int $scale = null): int
```

Compares two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bccomp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bccomp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bccomp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bccomp.md).
