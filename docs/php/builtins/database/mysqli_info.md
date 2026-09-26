---
title: "mysqli_info()"
description: "Returns information about the last query, when the server supplies it."
sidebar:
  order: 138
---

## mysqli_info()

```php
function mysqli_info(mixed $mysql): ?string
```

Returns information about the last query, when the server supplies it.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `?string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_info.md).
