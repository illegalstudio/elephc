---
title: "join()"
description: "Joins array elements into a single string using a separator (alias of implode)."
sidebar:
  order: 942
---

## join()

```php
function join(mixed $separator, mixed $array = null): string
```

Joins array elements into a single string using a separator (alias of implode).

**Parameters**:
- `$separator` (`mixed`)
- `$array` (`mixed`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/implode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/implode.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `join` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/join.md).
