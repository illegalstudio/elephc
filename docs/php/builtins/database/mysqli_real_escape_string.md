---
title: "mysqli_real_escape_string()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 148
---

## mysqli_real_escape_string()

```php
function mysqli_real_escape_string(mixed $mysql, string $string): string
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_real_escape_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_real_escape_string.md).
