---
title: "getimagesizefromstring()"
description: "Returns the size, type, and MIME type of an image held in a string."
sidebar:
  order: 455
---

## getimagesizefromstring()

```php
function getimagesizefromstring(string $data): mixed
```

Returns the size, type, and MIME type of an image held in a string.

**Parameters**:
- `$data` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getimagesizefromstring` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/getimagesizefromstring.md).
