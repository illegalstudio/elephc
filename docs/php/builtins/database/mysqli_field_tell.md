---
title: "mysqli_field_tell()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 126
---

## mysqli_field_tell()

```php
function mysqli_field_tell(mixed $result): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_field_tell` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_field_tell.md).
