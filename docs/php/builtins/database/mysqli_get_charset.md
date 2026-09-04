---
title: "mysqli_get_charset()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 128
---

## mysqli_get_charset()

```php
function mysqli_get_charset(mixed $mysql): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_charset` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_charset.md).
