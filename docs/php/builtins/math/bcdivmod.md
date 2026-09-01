---
title: "bcdivmod()"
description: "Returns the quotient and remainder of arbitrary-precision division."
sidebar:
  order: 272
---

## bcdivmod()

```php
function bcdivmod(string $num1, string $num2, int $scale = null): array
```

Returns the quotient and remainder of arbitrary-precision division.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcdivmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcdivmod.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcdivmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcdivmod.md).
