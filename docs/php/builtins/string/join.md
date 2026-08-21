---
title: "join()"
description: "Joins array elements into a single string using a separator (alias of implode)."
sidebar:
  order: 432
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
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `join` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/join.md).
