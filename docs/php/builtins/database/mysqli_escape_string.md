---
title: "mysqli_escape_string()"
description: "Alias of mysqli_real_escape_string()."
sidebar:
  order: 114
---

## mysqli_escape_string()

```php
function mysqli_escape_string(mixed $mysql, string $string): string
```

Alias of mysqli_real_escape_string().

**Parameters**:
- `$mysql` (`mixed`)
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_escape_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_escape_string.md).
