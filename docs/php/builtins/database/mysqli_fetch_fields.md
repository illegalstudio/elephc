---
title: "mysqli_fetch_fields()"
description: "Returns metadata for every column of a result."
sidebar:
  order: 123
---

## mysqli_fetch_fields()

```php
function mysqli_fetch_fields(mixed $result): array
```

Returns metadata for every column of a result.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_fields` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_fields.md).
