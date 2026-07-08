---
title: "preg_match()"
description: "Performs a regular expression match."
sidebar:
  order: 315
---

## preg_match()

```php
function preg_match(string $pattern, string $subject, array $matches = []): int
```

Performs a regular expression match.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`array`), passed by reference, default `[]`, optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match.md).

