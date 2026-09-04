---
title: "bcmul()"
description: "Multiplies two arbitrary-precision decimal numbers."
sidebar:
  order: 300
---

## bcmul()

```php
function bcmul(string $num1, string $num2, int $scale = null): string
```

Multiplies two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcmul.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcmul.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcmul` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcmul.md).
