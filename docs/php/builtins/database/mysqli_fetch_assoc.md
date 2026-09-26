---
title: "mysqli_fetch_assoc()"
description: "Returns the next row as an associative array."
sidebar:
  order: 119
---

## mysqli_fetch_assoc()

```php
function mysqli_fetch_assoc(mixed $result): ?array
```

Returns the next row as an associative array.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `?array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_assoc.md).
