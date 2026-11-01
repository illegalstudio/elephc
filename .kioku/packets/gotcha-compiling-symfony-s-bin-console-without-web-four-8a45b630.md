---
id: gotcha-compiling-symfony-s-bin-console-without-web-four-8a45b630
type: gotcha
title: "Compiling Symfony's bin/console without --web: four standard-PHP defects, one of them destructive"
description: "The console compiles in 57s once past a union stricter than its members, SCRIPT_FILENAME naming the executable, and readonly writes restricted to __construct; a superglobal reference in an array literal is still refused on purpose"
created: 2026-09-20
sources:
  - path: src/pipeline.rs
    blob: b2a484ef4f610c915e48b0886487ea148340f752
    lines: 105-140
    snip: 4cda15469729
    anchor: ".unwrap_or(raw)"
  - path: src/superglobals.rs
    blob: b685b2b4844e1c0e61522f508b6ff1fb552b63dc
    lines: 190-260
    snip: 24e586155210
    anchor: "pub fn seed_cli_populated_superglobals("
  - path: src/types/checker/stmt_check/assignments/properties.rs
    blob: 05b4670d2275bef8fc75dd43643b36aa2500e63a
    lines: 430-480
    snip: f546e003ecd5
    anchor: ".visible_property(property)"
  - path: src/codegen/lower_inst/globals_constants.rs
    blob: fd8ad3ca689b63c4fd0beef1974e478214fdf00f
    lines: 416-450
    snip: b23d4cdd9e41
    anchor: "fn lower_global_array_ref_marker("
---

# Compiling Symfony's bin/console without --web: four standard-PHP defects, one of them destructive

## Fact

Compiling `examples/symfony-app/bin/console` without `--web` takes **57 s** once it gets
through. Four defects stood in the way; every one reduced to a short standard-PHP probe, and
none of the fixes names a framework.

**0. `elephc bin/console` would have written its binary over its own source.** `file_stem` of
an extensionless input is the whole name, so the executable path IS the input path. There was
no guard. `src/pipeline.rs` now refuses before reading anything and names `--output-dir`.
Always build a console entry with `--output-dir`.

**1. A union parameter was stricter than any of its members.**

    function take(Cmd $c) {}                 // elephc accepts Object("")
    function takeUnion(callable|Cmd $c) {}   // elephc REFUSED Object("")

`$container->get($id)` is declared `?object` by PSR-11, so `Object("")|Void` is what every
eager service fetch produces, and `Application::addCommand(callable|Command $c)` refused it.
Fixed in `types_compatible` (src/types/checker/functions/call_validation.rs) by the rule its
neighbour already states for single heap shapes: a boxed value reaches a destination whose
members are all classes or `callable`, and the class is narrowed with a runtime check.

**2. `$_SERVER['SCRIPT_FILENAME']` named the executable, so the binary parsed ITSELF as PHP.**

    Parse error: syntax error, unexpected end of file in <the 108 MB binary> on line 599009

`vendor/autoload_runtime.php` does `$app = require $_SERVER['SCRIPT_FILENAME']` to collect the
closure the entry returns. `superglobals.rs` answered `$argv[0]` for the four path keys, on the
reasoning that "a compiled program has no script at run time". It has one: the entry it was
built from, which is what php reports and what the `--web` prelude already answers. Now
recorded per compile (`set_entry_script`, a thread-local beside `set_compiling_for_web`) and
emitted as a LITERAL -- `__FILE__` there panics, because the seeding rides on
`optimize::fold_constants` and magic constants are lowered before that.

**3. A readonly property could only be written from `__construct`.** php's rule is about SCOPE:
"initialized once, and only from the scope where they have been declared". Any method may do
it; the once-only half is dynamic. The constructor-only approximation rejected the ordinary
lazy idiom (`SymfonyRuntime::$input`, and this 20-line probe):

    private readonly \stdClass $slot;
    public function get(): \stdClass {
        if (isset($this->slot)) { return $this->slot; }
        return $this->slot = new \stdClass();
    }

**This fix trades a false positive for a false negative and that is not finished work.** elephc
has NO runtime readonly guard -- the check is entirely static -- so a genuine SECOND write from
inside the class is now accepted where php raises `Error`. Measured, all three cases:

    first write, non-constructor   php ok       elephc ok      (was fatal)
    second write, inside class     php Error    elephc ALLOWS  <-- the hole
    write from outside the class   php Error    elephc Error

Closing it means emitting an initialized-slot test at the write site and throwing, which is a
new mechanism in codegen.

**Still refused, deliberately: a reference to a superglobal inside an array literal.**
`['session' => &$_SESSION]` in `GenericRuntime::getArgument`. Lifting the `module.web` half of
the guard in `lower_global_array_ref_marker` and creating the cell lazily (as
`lower_global_ref_cell` does) COMPILES and then does not alias:

    $box = ['query' => &$_GET];  $box['query']['hit'] = 'x';
    php: $_GET['hit'] === 'x'    elephc: $_GET untouched

in a function body and at the top level alike, while plain superglobal reads and writes outside
`--web` are fine. The missing half is the consuming side: the `ARRAY_GLOBAL_REF_CELL_TAG`
marker is only honoured by the request-scoped storage `--web` installs.
`ELEPHC_BROKEN_CLI_SUPERGLOBAL_REF=1` compiles it anyway and warns on every use; it exists only
to reach the rest of a CLI entry point, which is how defects 2 and 3 were found.

**Next blocker:** `Call to undefined function error_reporting()`. The whole error-handling
surface is declared only in the `--web` prelude.

## Why

Each defect looked framework-specific and reduced to a probe of a few lines; and one fix trades a false positive for a false negative, which must not be mistaken for finished.
