---
title: "is_double()"
description: "Alias of is_float()."
sidebar:
  order: 562
---

## is_double()

```php
function is_double(mixed $value): bool
```

Alias of is_float().

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_double.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_double.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_double` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_double.md).
