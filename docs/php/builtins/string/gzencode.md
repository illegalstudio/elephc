---
title: "gzencode()"
description: "Compresses a string with the gzip framing."
sidebar:
  order: 463
---

## gzencode()

```php
function gzencode(string $data, int $level = -1, int $encoding = 31): mixed
```

Compresses a string with the gzip framing.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), default `-1`, optional
- `$encoding` (`int`), default `31`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzencode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzencode.md).
