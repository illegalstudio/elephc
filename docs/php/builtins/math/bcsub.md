---
title: "bcsub()"
description: "Subtracts two arbitrary-precision decimal numbers."
sidebar:
  order: 306
---

## bcsub()

```php
function bcsub(string $num1, string $num2, int $scale = null): string
```

Subtracts two arbitrary-precision decimal numbers.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcsub.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcsub.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcsub` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcsub.md).
