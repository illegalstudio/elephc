---
title: "imagepalettecopy()"
description: "Copies one image's palette onto another."
sidebar:
  order: 521
---

## imagepalettecopy()

```php
function imagepalettecopy(mixed $dst, mixed $src): bool
```

Copies one image's palette onto another.

**Parameters**:
- `$dst` (`mixed`)
- `$src` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagepalettecopy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepalettecopy.md).
