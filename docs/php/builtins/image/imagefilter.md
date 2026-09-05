---
title: "imagefilter()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 505
---

## imagefilter()

```php
function imagefilter(mixed $image, int $filter, int $arg1 = 0, int $arg2 = 0, int $arg3 = 0, int $arg4 = 0): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$filter` (`int`)
- `$arg1` (`int`), default `0`, optional
- `$arg2` (`int`), default `0`, optional
- `$arg3` (`int`), default `0`, optional
- `$arg4` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefilter` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilter.md).
