---
title: "exif_tagname()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 448
---

## exif_tagname()

```php
function exif_tagname(int $index): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$index` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_tagname` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_tagname.md).
