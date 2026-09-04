---
title: "bcdiv()"
description: "Divides two arbitrary-precision decimal numbers."
sidebar:
  order: 296
---

## bcdiv()

```php
function bcdiv(string $num1, string $num2, int $scale = null): string
```

Divides two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcdiv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcdiv` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcdiv.md).
