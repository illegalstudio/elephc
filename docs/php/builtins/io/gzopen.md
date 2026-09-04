---
title: "gzopen()"
description: "Opens a gz-file pointer on the zlib compression wrapper."
sidebar:
  order: 205
---

## gzopen()

```php
function gzopen(string $filename, string $mode, int $use_include_path = 0): mixed
```

Opens a gz-file pointer on the zlib compression wrapper.

**Parameters**:
- `$filename` (`string`)
- `$mode` (`string`)
- `$use_include_path` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzopen.md).
