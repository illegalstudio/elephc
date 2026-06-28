---
title: "file_put_contents()"
description: "Lowers `file_put_contents(path, data)` through the target-aware runtime writer."
sidebar:
  order: 146
---

## file_put_contents()

```php
function file_put_contents(string $filename, mixed $data, int $flags = 0, mixed $context = null): int
```

Lowers `file_put_contents(path, data)` through the target-aware runtime writer.

**Parameters**:
- `$filename` (`string`)
- `$data` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_put_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_put_contents.md).

