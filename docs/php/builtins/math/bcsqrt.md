---
title: "bcsqrt()"
description: "Returns the square root of an arbitrary-precision decimal number."
sidebar:
  order: 291
---

## bcsqrt()

```php
function bcsqrt(string $num, int $scale = null): string
```

Returns the square root of an arbitrary-precision decimal number.

**Parameters**:
- `$num` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcsqrt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcsqrt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcsqrt` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcsqrt.md).
