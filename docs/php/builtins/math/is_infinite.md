---
title: "is_infinite()"
description: "Checks whether a float is infinite."
sidebar:
  order: 269
---

## is_infinite()

```php
function is_infinite(float $num): bool
```

Checks whether a float is infinite.

**Parameters**:
- `$num` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_infinite.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_infinite.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_infinite` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/is_infinite.md).

