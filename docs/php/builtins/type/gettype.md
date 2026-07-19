---
title: "gettype()"
description: "Returns the type of a variable as a string."
sidebar:
  order: 423
---

## gettype()

```php
function gettype(mixed $value): string
```

Returns the type of a variable as a string.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/gettype.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/gettype.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gettype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/gettype.md).

