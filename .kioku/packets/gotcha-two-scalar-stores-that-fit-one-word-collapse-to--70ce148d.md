---
id: gotcha-two-scalar-stores-that-fit-one-word-collapse-to--70ce148d
type: gotcha
title: "Two scalar stores that fit one word collapse to the last store's type"
description: "A local written bool on one path and int on another takes the type of the LAST store in program order, not the one that ran, so php's bool(true) prints int(1) and the reverse also fails. bool|string is correct, which shows the discriminator is a union foldable into one machine word losing its runtime tag. Three lines of plain PHP, no includes"
created: 2026-09-21
verified_by: "Six-case probe beside php 8.5.10 on a frozen pre-change compiler; both directions wrong, three controls correct including bool|string"
sources:
  - path: src/types/checker/stmt_check/control_flow.rs
    blob: cb8ed999f1f6ddad5d1d2b93603ddbeeed97f2c4
---

# Two scalar stores that fit one word collapse to the last store's type

## Fact

Two scalar stores to one local, on branches php never joins, COLLAPSE INTO ONE SLOT TYPED BY THE
LAST STORE IN PROGRAM ORDER — so the value carries the type of a store that never ran. Three
lines of plain PHP, no includes, no classes, no framework.

MEASURED against a frozen pre-change compiler, php 8.5.10 beside it
(`scratchpad/unioncollapse/p.php`, `run.sh`):

    $a = true;  if (never()) { $a = 1;    }   php bool(true)   elephc int(1)     WRONG
    $c = 1;     if (never()) { $c = true; }   php int(1)       elephc bool(true) WRONG
    $b = true;  if (always()){ $b = 1;    }   php int(1)       elephc int(1)     ok
    $d = true;  if (never()) { $d = false;}   php bool(true)   elephc bool(true) ok
    $e = true;  if (never()) { $e = 'x';  }   php bool(true)   elephc bool(true) ok
    $f = true;  $f = 1;                       php int(1)       elephc int(1)     ok

IT IS SYMMETRIC — bool-then-int and int-then-bool are both wrong, each taking the other's type.
An earlier report described only the first direction.

THE DISCRIMINATOR IS ONE MACHINE WORD. `bool|string` is CORRECT, because the checker cannot fold
those into a single word and keeps a tagged representation. `bool|int` is wrong, because it can.
So the bug is not "unions are mishandled"; it is that a union foldable into one word loses the
runtime tag, and the tag then follows the SLOT's static type instead of the store that executed.

WHY IT MATTERS BEYOND var_dump: `true` and `1` are `==` but not `===`, and php code gates on the
strict form constantly. `vendor/autoload_runtime.php:5` is literally
`if (true === (require_once __DIR__.'/autoload.php'))` — a program can be steered onto the wrong
branch by this with no diagnostic anywhere.

IT BLOCKS A PERFORMANCE FIX, which is how it was found. Generalising `try_expand_value_include`
to hoist a value-position include out of an `if` condition moves that guard from the interpreted
path (which answers `true` correctly) onto the compiled one, where this collapse would answer
`1`. The order is therefore: fix the repeat-`require_once` value first, then this, then the
hoist — not the other way round.

WHERE TO LOOK: the local-slot typing that unifies stores across branches. The IR is provably
right — an `--emit-ir` dump shows `store_local const_bool true` before the guard and
`store_local const_i64 1` inside its body — so the loss happens below the IR, in how the slot's
declared type decides the tag on read. `kioku recall "promoted local is boxed mixed"` and
`kioku recall "a short-circuit operand's local-type facts must be JOINED"` are the neighbouring
packets on local typing.

## Why

true and 1 are == but not ===, and real code gates on the strict form -- vendor/autoload_runtime.php literally writes if (true === (require_once ...)). A program can take the wrong branch with no diagnostic anywhere. It also blocks hoisting a value-include out of an if condition, because that moves the guard onto the compiled path where the collapse happens.
