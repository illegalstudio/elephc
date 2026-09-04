---
title: "image_type_to_extension()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 453
---

## image_type_to_extension()

```php
function image_type_to_extension(int $image_type, bool $include_dot = true): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image_type` (`int`)
- `$include_dot` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `image_type_to_extension` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/image_type_to_extension.md).
