---
title: "is_finite()"
description: "Checks whether a float is finite."
sidebar:
  order: 268
---

## is_finite()

```php
function is_finite(float $num): bool
```

Checks whether a float is finite.

**Parameters**:
- `$num` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_finite.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_finite.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_finite` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/is_finite.md).

