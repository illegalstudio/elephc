---
title: "imagepalettecopy()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 518
---

## imagepalettecopy()

```php
function imagepalettecopy(mixed $dst, mixed $src): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$dst` (`mixed`)
- `$src` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagepalettecopy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepalettecopy.md).
