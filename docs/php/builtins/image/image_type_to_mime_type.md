---
title: "image_type_to_mime_type()"
description: "Returns the MIME type for an IMAGETYPE_* constant."
sidebar:
  order: 457
---

## image_type_to_mime_type()

```php
function image_type_to_mime_type(int $image_type): string
```

Returns the MIME type for an IMAGETYPE_* constant.

**Parameters**:
- `$image_type` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `image_type_to_mime_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/image_type_to_mime_type.md).
