---
title: "mysqli_get_proto_info()"
description: "Returns the MySQL protocol version in use."
sidebar:
  order: 135
---

## mysqli_get_proto_info()

```php
function mysqli_get_proto_info(mixed $mysql): int
```

Returns the MySQL protocol version in use.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_proto_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_proto_info.md).
