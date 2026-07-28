---
title: "intval()"
description: "Returns the integer value of a variable."
sidebar:
  order: 446
---

## intval()

```php
function intval(mixed $value): int
```

Returns the integer value of a variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/intval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/intval.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `intval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/intval.md).
