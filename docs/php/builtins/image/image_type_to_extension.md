---
title: "image_type_to_extension()"
description: "Returns the filename extension for an IMAGETYPE_* constant."
---

## image_type_to_extension()

```php
function image_type_to_extension(int $image_type, bool $include_dot = true): mixed
```

Returns the filename extension for an IMAGETYPE_* constant.

**Parameters**:
- `$image_type` (`int`)
- `$include_dot` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `image_type_to_extension` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/image_type_to_extension.md).
