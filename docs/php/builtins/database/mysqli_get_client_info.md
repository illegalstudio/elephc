---
title: "mysqli_get_client_info()"
description: "Returns the client library version as a string."
sidebar:
  order: 132
---

## mysqli_get_client_info()

```php
function mysqli_get_client_info(mixed $mysql): string
```

Returns the client library version as a string.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_client_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_client_info.md).
