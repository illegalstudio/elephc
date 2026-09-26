---
title: "mysqli_fetch_object()"
description: "Returns the next row as an object."
sidebar:
  order: 125
---

## mysqli_fetch_object()

```php
function mysqli_fetch_object(mixed $result, string $class = 'stdClass', array $constructor_args = []): mixed
```

Returns the next row as an object.

**Parameters**:
- `$result` (`mixed`)
- `$class` (`string`), default `'stdClass'`, optional
- `$constructor_args` (`array`), default `[]`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_object.md).
