---
title: "mysqli_get_client_version()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 130
---

## mysqli_get_client_version()

```php
function mysqli_get_client_version(mixed $mysql): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_client_version` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_client_version.md).
