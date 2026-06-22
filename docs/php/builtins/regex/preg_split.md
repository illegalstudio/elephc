---
title: "preg_split()"
description: "Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper."
sidebar:
  order: 300
---

## preg_split()

```php
function preg_split(string $pattern, string $subject, int $limit, int $flags): array
```

Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$limit` (`int`), optional
- `$flags` (`int`), optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_split.md).

