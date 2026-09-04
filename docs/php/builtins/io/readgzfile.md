---
title: "readgzfile()"
description: "Outputs a gz-file and answers the byte count."
sidebar:
  order: 231
---

## readgzfile()

```php
function readgzfile(string $filename, int $use_include_path = 0): mixed
```

Outputs a gz-file and answers the byte count.

**Parameters**:
- `$filename` (`string`)
- `$use_include_path` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `readgzfile` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/readgzfile.md).
