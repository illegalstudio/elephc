---
title: "isset()"
description: "Determines whether a variable is set and is not null."
sidebar:
  order: 262
---

## isset()

```php
function isset(mixed $var, ...$vars): int
```

Determines whether a variable is set and is not null.

**Parameters**:
- `$var` (`mixed`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `isset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/isset.md).

