---
title: "empty()"
description: "Determines whether a variable is considered empty."
sidebar:
  order: 293
---

## empty()

```php
function empty(mixed $value): bool
```

Determines whether a variable is considered empty.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `empty` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/empty.md).
