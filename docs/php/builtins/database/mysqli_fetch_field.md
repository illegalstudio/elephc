---
title: "mysqli_fetch_field()"
description: "Returns metadata for the next column of a result."
sidebar:
  order: 121
---

## mysqli_fetch_field()

```php
function mysqli_fetch_field(mixed $result): mixed
```

Returns metadata for the next column of a result.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_field` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_field.md).
