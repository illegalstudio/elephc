---
title: "preg_replace()"
description: "Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper."
sidebar:
  order: 296
---

## preg_replace()

```php
function preg_replace(string $pattern, string $replacement, string $subject, int $limit = -1, int $count = null): string
```

Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$subject` (`string`)
- `$limit` (`int`), default `-1`, optional
- `$count` (`int`), passed by reference, default `null`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace.md).

