---
title: "mysqli_set_charset()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 155
---

## mysqli_set_charset()

```php
function mysqli_set_charset(mixed $mysql, string $charset): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$charset` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_set_charset` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_set_charset.md).
