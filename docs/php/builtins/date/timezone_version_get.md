---
title: "timezone_version_get()"
description: "Returns the version of the bundled timezone database."
sidebar:
  order: 254
---

## timezone_version_get()

```php
function timezone_version_get(): string
```

Returns the version of the bundled timezone database.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_version_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_version_get.md).
