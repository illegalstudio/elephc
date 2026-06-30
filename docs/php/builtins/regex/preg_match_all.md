---
title: "preg_match_all()"
description: "Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper."
sidebar:
  order: 312
---

## preg_match_all()

```php
function preg_match_all(string $pattern, string $subject, array $matches): int
```

Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`array`), passed by reference

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_match_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match_all.md).

