---
title: "strval()"
description: "Gets the string value of a variable."
sidebar:
  order: 457
---

## strval()

```php
function strval(mixed $value): string
```

Gets the string value of a variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/strval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/strval.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/strval.md).
