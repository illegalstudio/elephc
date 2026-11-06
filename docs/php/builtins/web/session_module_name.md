---
title: "session_module_name()"
description: "Reads or sets the session save handler module."
---

## session_module_name()

```php
function session_module_name(?string $module = null): mixed
```

Reads or sets the session save handler module.

**Parameters**:
- `$module` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_module_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_module_name.md).
