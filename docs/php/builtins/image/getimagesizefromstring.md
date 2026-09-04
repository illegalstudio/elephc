---
title: "getimagesizefromstring()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 452
---

## getimagesizefromstring()

```php
function getimagesizefromstring(string $data): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$data` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getimagesizefromstring` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/getimagesizefromstring.md).
