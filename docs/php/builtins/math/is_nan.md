---
title: "is_nan()"
description: "Checks whether a float is NAN."
sidebar:
  order: 272
---

## is_nan()

```php
function is_nan(float $num): bool
```

Checks whether a float is NAN.

**Parameters**:
- `$num` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_nan.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_nan.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_nan` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/is_nan.md).
