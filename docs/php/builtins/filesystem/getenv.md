---
title: "getenv()"
description: "Gets the value of an environment variable, or the whole environment."
---

## getenv()

```php
function getenv(?string $name = null, bool $local_only = false): string|array|false
```

Gets the value of an environment variable, or the whole environment.

**Parameters**:
- `$name` (`?string`), default `null`, optional
- `$local_only` (`bool`), default `false`, optional

**Returns**: `string|array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/getenv.md).
