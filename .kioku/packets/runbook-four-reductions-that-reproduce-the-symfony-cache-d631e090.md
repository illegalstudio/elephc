---
id: runbook-four-reductions-that-reproduce-the-symfony-cache-d631e090
type: runbook
title: "Four reductions that reproduce the symfony/cache blockers, with their php -n oracles"
description: "Each of the remaining families as a runnable file, so the fix has a test before it is written"
tags: [symfony, checker, reductions]
created: 2026-09-16
verified_by: "each file run under php -n and through elephc; the diagnostics match the ones the max preload reports"
sources:
  - path: examples/symfony-app/public/index_preload_max.php
    blob: 042917593b8e769b60d600cf42398e474b9eb42a
---

# Four reductions that reproduce the symfony/cache blockers, with their php -n oracles

## Fact

The 23 errors that keep `symfony/cache` out of the `--web` preload reduce to four files. They live
in the session scratchpad; re-create them from this description if it is gone.

1. BY-REFERENCE ARGUMENTS THAT DO NOT DEFINE THEIR VARIABLE (11 of the 23), 57 lines, three shapes:
   - a union receiver: `class Conn { function prepare(string $s): Stmt|false }`, then
     `$stmt->bind(':id', $id); $id = 42;` — `bind` takes `&$value` and stores `$this->bound[$n] = &$value`.
     PHP prints `[42,"blob",{":id":42,":data":"blob"}]`: the binding is LIVE, and the value assigned
     AFTER the bind is what the callee sees. A fix that merely defines the variable gets the type
     right and the value wrong.
   - a `\Closure` in a static property: `(self::$merge)($this->deferred, $expiredIds)` where the
     closure declares `&$expiredIds`. Two errors: the call AND the read below it.
   - `preg_match($re, $subject, $m)` — this one already WORKS, so the prelude path is fine and the
     hole is exactly "callee signature not resolvable from a plain object receiver".
   elephc: `Undefined variable: $id / $data / $expiredIds (x2)`.

2. A REFERENCE-ALIASED LOCAL THAT CHANGES TYPE, 20 lines: `$error` is captured `use (&$error)` by an
   error handler, is then BOTH the subject and the `$matches` out-parameter of one `preg_match`, and
   is finally assigned a string. PHP prints `1: … 2:  (failed) 3: still-same 4: y 5: z`.
   elephc: `cannot reassign $error from array<mixed> to string`.
   The rule: a reference-aliased local is a box with STABLE IDENTITY; assignment updates the cell,
   it never rebinds the name to a new cell. elephc's refusal is right in principle — what is missing
   is letting the type change go THROUGH the box.

3. A `: bool` METHOD THAT CAN FALL OFF THE END, 23 lines. PHP compiles it and raises
   `TypeError: C::clear(): Return value must be of type bool, null returned` at the moment that
   path returns. elephc refuses at compile time: `must return a value on every path`.

4. A UNION RECEIVER WHOSE OBJECT REACHES `__call`, 55 lines — found while chasing the 7 "nullsafe"
   errors and NOT the same defect: it compiles and then MISCOMPILES.
   PHP `elastic:p1,elastic:p2`; elephc prints nothing and dies with
   `Call to a member function load() on null`.

On the 7 "nullsafe method call requires a single nullable object type" errors themselves: the
message is a red herring. `nullsafe_object_receiver` is the shared "give me the one class of this
receiver" helper, used by ORDINARY calls too, and it refuses a union with two distinct object
members. But the real `RedisTrait` compiled ALONE reports zero errors, and a union naming classes
that are never declared compiles too — so the trigger is the trait being flattened into the class,
or the surrounding closed world, not the union declaration.
