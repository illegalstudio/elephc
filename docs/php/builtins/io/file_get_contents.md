---
title: "file_get_contents()"
description: "Lowers `file_get_contents(path)` and boxes the runtime string-or-false result."
sidebar:
  order: 145
---

## file_get_contents()

```php
function file_get_contents(string $filename, bool $use_include_path, mixed $context, int $offset, int $length): mixed
```

Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.

**Parameters**:
- `$filename` (`string`)
- `$use_include_path` (`bool`)
- `$context` (`mixed`)
- `$offset` (`int`)
- `$length` (`int`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_get_contents.md).

