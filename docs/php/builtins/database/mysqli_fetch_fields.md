---
title: "mysqli_fetch_fields()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 120
---

## mysqli_fetch_fields()

```php
function mysqli_fetch_fields(mixed $result): array
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_fields` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_fields.md).
