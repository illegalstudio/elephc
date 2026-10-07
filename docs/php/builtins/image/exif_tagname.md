---
title: "exif_tagname()"
description: "Returns the name of an EXIF tag index."
sidebar:
  order: 453
---

## exif_tagname()

```php
function exif_tagname(int $index): mixed
```

Returns the name of an EXIF tag index.

**Parameters**:
- `$index` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_tagname` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_tagname.md).
