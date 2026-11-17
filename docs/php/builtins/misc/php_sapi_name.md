---
title: "php_sapi_name()"
description: "Returns the name of the server API this build runs under."
---

## php_sapi_name()

```php
function php_sapi_name(): string
```

Returns the name of the server API this build runs under.

**Parameters**: none.

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `php_sapi_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/php_sapi_name.md).
