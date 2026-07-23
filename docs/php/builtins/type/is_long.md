---
title: "is_long()"
description: "Alias of is_int()."
sidebar:
  order: 455
---

## is_long()

```php
function is_long(mixed $value): bool
```

Alias of is_int().

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_long.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_long.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_long` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_long.md).
