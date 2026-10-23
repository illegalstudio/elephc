---
title: "mysqli_get_server_version()"
description: "Returns the server version as an integer."
sidebar:
  order: 136
---

## mysqli_get_server_version()

```php
function mysqli_get_server_version(mixed $mysql): int
```

Returns the server version as an integer.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_server_version` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_server_version.md).
