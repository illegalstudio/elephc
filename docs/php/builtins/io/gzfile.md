---
title: "gzfile()"
description: "Reads an entire gz-file into an array of lines."
sidebar:
  order: 202
---

## gzfile()

```php
function gzfile(string $filename, int $use_include_path = 0): mixed
```

Reads an entire gz-file into an array of lines.

**Parameters**:
- `$filename` (`string`)
- `$use_include_path` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzfile` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzfile.md).
