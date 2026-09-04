---
title: "mysqli_fetch_object()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 122
---

## mysqli_fetch_object()

```php
function mysqli_fetch_object(mixed $result, string $class = 'stdClass', mixed $constructor_args = []): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)
- `$class` (`string`), default `'stdClass'`, optional
- `$constructor_args` (`mixed`), default `[]`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_object.md).
