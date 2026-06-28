---
title: "fopen()"
description: "Lowers `fopen(filename, mode)` and boxes stream resources or PHP false."
sidebar:
  order: 148
---

## fopen()

```php
function fopen(string $filename, string $mode, bool $use_include_path, mixed $context): mixed
```

Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.

**Parameters**:
- `$filename` (`string`)
- `$mode` (`string`)
- `$use_include_path` (`bool`), optional
- `$context` (`mixed`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fopen.md).

