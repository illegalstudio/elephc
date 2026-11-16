---
title: "token_name()"
description: "Get the symbolic name of a PHP token identifier."
---

## token_name()

```php
function token_name(int $id): string
```

Get the symbolic name of a PHP token identifier.

**Parameters**:
- `$id` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `token_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/token_name.md).
