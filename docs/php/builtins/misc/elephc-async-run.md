---
title: "elephc\async\run()"
description: "Runs a structured cooperative task scope to completion."
sidebar:
  order: 609
---

## elephc\async\run()

```php
function elephc\async\run(callable $body): mixed
```

Runs a structured cooperative task scope to completion.

**Parameters**:
- `$body` (`callable`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.
- **Strict PHP mode**: hidden - this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

**Examples**:

examples/async-scheduler/main.php

## Internals

For how `elephc\async\run` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/elephc-async-run.md).
