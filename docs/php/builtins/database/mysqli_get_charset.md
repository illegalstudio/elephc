---
title: "mysqli_get_charset()"
description: "Returns the connection character set as an object."
sidebar:
  order: 131
---

## mysqli_get_charset()

```php
function mysqli_get_charset(mixed $mysql): mixed
```

Returns the connection character set as an object.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_charset` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_charset.md).
