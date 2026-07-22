---
title: "is_integer()"
description: "Alias of is_int()."
sidebar:
  order: 446
---

## is_integer()

```php
function is_integer(mixed $value): bool
```

Alias of is_int().

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_integer.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_integer.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_integer` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_integer.md).
