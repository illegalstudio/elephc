---
title: "mysqli_get_host_info()"
description: "Returns the server host name and connection type."
sidebar:
  order: 134
---

## mysqli_get_host_info()

```php
function mysqli_get_host_info(mixed $mysql): string
```

Returns the server host name and connection type.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_host_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_host_info.md).
