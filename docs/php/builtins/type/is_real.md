---
title: "is_real()"
description: "Alias of is_float()."
sidebar:
  order: 452
---

## is_real()

```php
function is_real(mixed $value): bool
```

Alias of is_float().

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_real.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_real.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_real` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_real.md).
