---
title: "ini_get_all()"
description: "Returns every configuration directive, optionally with its access level and scope."
sidebar:
  order: 913
---

## ini_get_all()

```php
function ini_get_all(?string $extension = null, bool $details = true): array|false
```

Returns every configuration directive, optionally with its access level and scope.

**Parameters**:
- `$extension` (`?string`), default `null`, optional
- `$details` (`bool`), default `true`, optional

**Returns**: `array|false`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected shared_ini prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_get_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_get_all.md).
