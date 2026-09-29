---
title: "mysqli_set_charset()"
description: "Sets the character set used by the connection."
sidebar:
  order: 158
---

## mysqli_set_charset()

```php
function mysqli_set_charset(mixed $mysql, string $charset): bool
```

Sets the character set used by the connection.

**Parameters**:
- `$mysql` (`mixed`)
- `$charset` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_set_charset` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_set_charset.md).
