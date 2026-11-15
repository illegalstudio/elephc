//! Purpose:
//! Integration tests for PHP's `goto` statement and its labels, compiled end to end and compared
//! against PHP's output.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was produced by running the same fixture under `LC_ALL=C php`
//!   (PHP 8.5.12).
//! - `goto` is lowered to dispatch loops before type checking, so the fixtures concentrate on
//!   where a structured rewrite could differ from a real jump: leaving loops, `switch`es and
//!   `try` blocks, entering `if` and `try` bodies, `finally` ordering, the `break`/`continue`
//!   levels of the surrounding code, locals whose assignment a jump skips, included files, and
//!   `eval` fragments, which the runtime interpreter executes with its own `goto`.

use super::*;

/// Verifies a backward jump to a leading label repeats the code between them.
#[test]
fn test_goto_backward_loop() {
    let out = compile_and_run(
        r#"<?php
        $i = 0;
        again:
        echo $i;
        $i++;
        if ($i < 3) goto again;
        echo " done";
        "#,
    );
    assert_eq!(out, "012 done");
}

/// Verifies a forward jump skips the statements before its label.
#[test]
fn test_goto_forward_skip() {
    let out = compile_and_run(
        r#"<?php
        echo "a";
        goto end;
        echo "skipped";
        end:
        echo "b";
        "#,
    );
    assert_eq!(out, "ab");
}

/// Verifies the keyword is case-insensitive while the label keeps its spelling.
#[test]
fn test_goto_keyword_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
        GOTO Done;
        echo "skipped";
        Done:
        echo "ok";
        "#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a function whose last segment returns still returns on every path.
#[test]
fn test_goto_function_with_return_type() {
    let out = compile_and_run(
        r#"<?php
        function next_even(int $n): int {
            $i = 0;
            step:
            $i += 2;
            if ($i < $n) goto step;
            return $i;
        }
        echo next_even(7), ",", next_even(1);
        "#,
    );
    assert_eq!(out, "8,2");
}

/// Verifies a jump out of two nested loops lands on the label after them.
#[test]
fn test_goto_out_of_nested_loops() {
    let out = compile_and_run(
        r#"<?php
        for ($i = 1; $i < 5; $i++) {
            for ($j = 1; $j < 5; $j++) {
                if ($i * $j === 6) goto found;
            }
        }
        echo "none";
        goto done;
        found:
        echo "found $i*$j";
        done:
        echo ".";
        "#,
    );
    assert_eq!(out, "found 2*3.");
}

/// Verifies a jump out of a foreach keeps the loop variables it had.
#[test]
fn test_goto_out_of_foreach() {
    let out = compile_and_run(
        r#"<?php
        $total = 0;
        foreach ([3, 5, 8, 13] as $k => $v) {
            $total += $v;
            if ($total > 10) goto over;
        }
        echo "under";
        over:
        echo "k=$k v=$v total=$total";
        "#,
    );
    assert_eq!(out, "k=2 v=8 total=16");
}

/// Verifies code after an infinite loop is reachable through a jump out of it.
#[test]
fn test_goto_out_of_infinite_loop() {
    let out = compile_and_run(
        r#"<?php
        $n = 0;
        while (true) {
            $n++;
            if ($n === 4) goto out;
        }
        out:
        echo "n=$n";
        "#,
    );
    assert_eq!(out, "n=4");
}

/// Verifies the retry idiom: a catch block jumps back to a label before its try.
#[test]
fn test_goto_retry_from_catch() {
    let out = compile_and_run(
        r#"<?php
        $attempt = 0;
        retry:
        try {
            $attempt++;
            if ($attempt < 3) {
                throw new Exception("fail $attempt");
            }
            echo "ok after $attempt";
        } catch (Exception $e) {
            echo $e->getMessage(), ";";
            goto retry;
        }
        "#,
    );
    assert_eq!(out, "fail 1;fail 2;ok after 3");
}

/// Verifies a jump out of a try block runs its finally block first.
#[test]
fn test_goto_out_of_try_runs_finally() {
    let out = compile_and_run(
        r#"<?php
        for ($i = 0; $i < 2; $i++) {
            try {
                echo "t$i ";
                goto out;
            } finally {
                echo "fin ";
            }
        }
        out:
        echo "out";
        "#,
    );
    assert_eq!(out, "t0 fin out");
}

/// Verifies a jump whose label is in the same finally block stays inside it.
#[test]
fn test_goto_within_finally() {
    let out = compile_and_run(
        r#"<?php
        try {
            echo "body ";
        } finally {
            $i = 0;
            again:
            $i++;
            if ($i < 3) goto again;
            echo "f$i";
        }
        "#,
    );
    assert_eq!(out, "body f3");
}

/// Verifies a jump into an if branch skips the condition and the code before the label.
#[test]
fn test_goto_into_if_branch() {
    let out = compile_and_run(
        r#"<?php
        $taken = false;
        goto inside;
        if ($taken) {
            echo "skipped ";
            inside:
            echo "in-if ";
        } else {
            echo "else ";
        }
        echo "after";
        "#,
    );
    assert_eq!(out, "in-if after");
}

/// Verifies a jump into an else branch does not evaluate the if condition.
#[test]
fn test_goto_into_else_branch() {
    let out = compile_and_run(
        r#"<?php
        function side(): bool { echo "side "; return true; }
        goto inside;
        if (side()) {
            echo "then ";
        } else {
            echo "skipped ";
            inside:
            echo "in-else ";
        }
        echo "after";
        "#,
    );
    assert_eq!(out, "in-else after");
}

/// Verifies a jump into an elseif branch skips every earlier condition.
#[test]
fn test_goto_into_elseif_branch() {
    let out = compile_and_run(
        r#"<?php
        function probe(string $name): bool { echo "[$name]"; return false; }
        $round = 0;
        start:
        $round++;
        if (probe("a$round")) {
            echo "A";
        } elseif (probe("b$round")) {
            echo "B";
            middle:
            echo "M$round ";
        } else {
            echo "E$round ";
        }
        $round++;
        if ($round === 2) goto middle;
        if ($round === 3) goto start;
        echo "end";
        "#,
    );
    assert_eq!(out, "[a1][b1]E1 M2 [a4][b4]E4 end");
}

/// Verifies a jump into a try body installs the handler: a later throw is caught.
#[test]
fn test_goto_into_try_body() {
    let out = compile_and_run(
        r#"<?php
        goto inside;
        try {
            echo "skipped ";
            inside:
            echo "in-try ";
            throw new Exception("x");
        } catch (Exception $e) {
            echo "caught ";
        } finally {
            echo "fin";
        }
        "#,
    );
    assert_eq!(out, "in-try caught fin");
}

/// Verifies a catch jumping back into its own try body does not run finally early.
#[test]
fn test_goto_catch_to_try_label_runs_finally_once() {
    let out = compile_and_run(
        r#"<?php
        $n = 0;
        try {
            again:
            $n++;
            echo "try$n ";
            if ($n < 3) throw new Exception("e");
            echo "ok ";
        } catch (Exception $e) {
            echo "catch$n ";
            goto again;
        } finally {
            echo "fin";
        }
        "#,
    );
    assert_eq!(out, "try1 catch1 try2 catch2 try3 ok fin");
}

/// Verifies a label inside a loop body and the loop's own break and continue keep their targets.
#[test]
fn test_goto_label_inside_loop_body() {
    let out = compile_and_run(
        r#"<?php
        for ($i = 0; $i < 6; $i++) {
            $tries = 0;
            again:
            $tries++;
            if ($i === 1) continue;
            if ($i === 4) break;
            if ($tries < 2) goto again;
            echo "$i:$tries ";
        }
        echo "end";
        "#,
    );
    assert_eq!(out, "0:2 2:2 3:2 end");
}

/// Verifies a jump out of a switch inside a loop leaves both.
#[test]
fn test_goto_out_of_switch() {
    let out = compile_and_run(
        r#"<?php
        foreach ([1, 2, 3] as $v) {
            switch ($v) {
                case 1:
                    echo "one ";
                    break;
                case 2:
                    echo "two ";
                    goto out;
                default:
                    echo "other ";
            }
        }
        out:
        echo "out";
        "#,
    );
    assert_eq!(out, "one two out");
}

/// Verifies a label and its jump inside one case body, with the case's own break.
#[test]
fn test_goto_within_switch_case() {
    let out = compile_and_run(
        r#"<?php
        function count_to(int $kind): string {
            $out = "";
            switch ($kind) {
                case 1:
                    $i = 0;
                    more:
                    $i++;
                    $out .= $i;
                    if ($i < 3) goto more;
                    break;
                default:
                    $out = "default";
            }
            return $out . "!";
        }
        echo count_to(1), count_to(2);
        "#,
    );
    assert_eq!(out, "123!default!");
}

/// Verifies several labels reached in an order different from their source order.
#[test]
fn test_goto_state_machine() {
    let out = compile_and_run(
        r#"<?php
        $trace = "";
        goto first;
        third:
        $trace .= "3";
        goto done;
        second:
        $trace .= "2";
        goto third;
        first:
        $trace .= "1";
        goto second;
        done:
        echo $trace;
        "#,
    );
    assert_eq!(out, "123");
}

/// Verifies an outer label and a label inside a nested loop body route independently.
#[test]
fn test_goto_nested_dispatch() {
    let out = compile_and_run(
        r#"<?php
        $outer = 0;
        top:
        $outer++;
        for ($i = 0; $i < 2; $i++) {
            $inner = 0;
            again:
            $inner++;
            if ($inner < 2) goto again;
            echo "$outer.$i.$inner ";
            if ($outer < 2 && $i === 1) goto top;
        }
        echo "end";
        "#,
    );
    assert_eq!(out, "1.0.2 1.1.2 2.0.2 2.1.2 end");
}

/// Verifies closures and methods are label scopes of their own.
#[test]
fn test_goto_closure_and_method_scopes() {
    let out = compile_and_run(
        r#"<?php
        class Counter {
            public function upTo(int $n): string {
                $s = "";
                $i = 0;
                next:
                $i++;
                $s .= $i;
                if ($i < $n) goto next;
                return $s;
            }
        }
        $twice = function (string $s): string {
            $n = 0;
            $out = "";
            next:
            $out .= $s;
            $n++;
            if ($n < 2) goto next;
            return $out;
        };
        next:
        echo (new Counter())->upTo(3), " ", $twice("ab");
        "#,
    );
    assert_eq!(out, "123 abab");
}

/// Verifies an unused label and a label that ends a function body are accepted.
#[test]
fn test_goto_unused_label_and_label_at_end() {
    let out = compile_and_run(
        r#"<?php
        function f(): void {
            unused:
            $s = "a";
            goto tail;
            $s = "skipped";
            tail:
        }
        f();
        echo "ok";
        "#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a jump inside a generator keeps yielding across the dispatch loop.
#[test]
fn test_goto_generator_body() {
    let out = compile_and_run(
        r#"<?php
        function gen() {
            $i = 0;
            more:
            $i++;
            yield $i;
            if ($i < 3) goto more;
            yield "end";
        }
        foreach (gen() as $v) {
            echo $v, " ";
        }
        "#,
    );
    assert_eq!(out, "1 2 3 end ");
}

/// Verifies jumps in different namespaces of one file are independent: the `namespace` and
/// `use` declarations between them keep applying to the code written after them, and each
/// namespace keeps its own function of the same name.
#[test]
fn test_goto_independent_jumps_in_two_namespaces() {
    let out = compile_and_run(
        r#"<?php
        namespace A;
        function f() { return "A"; }
        goto a;
        echo "skipped";
        a:
        echo f();
        namespace B;
        use function A\f as af;
        function f() { return "B"; }
        goto b;
        echo "skipped";
        b:
        echo f(), af();
        "#,
    );
    assert_eq!(out, "ABA");
}

/// Verifies a jump inside a braced `namespace` block is lowered inside that block, which keeps
/// resolving its own names.
#[test]
fn test_goto_inside_braced_namespace_block() {
    let out = compile_and_run(
        r#"<?php
        namespace A {
            function f() { return "A"; }
            $i = 0;
            again:
            $i++;
            if ($i < 3) goto again;
            echo f(), $i;
        }
        namespace B {
            function f() { return "B"; }
            echo f();
        }
        "#,
    );
    assert_eq!(out, "A3B");
}

/// Verifies a file with a label and no `goto` at all compiles and ignores the label.
#[test]
fn test_goto_label_without_any_jump() {
    let out = compile_and_run(
        r#"<?php
        echo "a";
        marker:
        echo "b";
        "#,
    );
    assert_eq!(out, "ab");
}

/// Verifies a `const` written between two independent jumps is defined once, in order.
#[test]
fn test_goto_const_between_independent_jumps() {
    let out = compile_and_run(
        r#"<?php
        $i = 0;
        first:
        $i++;
        if ($i < 2) goto first;
        const LIMIT = 4;
        second:
        $i++;
        if ($i < LIMIT) goto second;
        echo $i;
        "#,
    );
    assert_eq!(out, "4");
}

/// Verifies a function declared between a label and its jump is still declared.
#[test]
fn test_goto_declaration_between_label_and_jump() {
    let out = compile_and_run(
        r#"<?php
        $n = 0;
        again:
        $n++;
        function helper(int $n): string { return "h$n "; }
        echo helper($n);
        if ($n < 2) goto again;
        echo "end";
        "#,
    );
    assert_eq!(out, "h1 h2 end");
}

/// Verifies the jump state is not a PHP variable: it is absent from get_defined_vars().
#[test]
fn test_goto_state_local_is_hidden() {
    let out = compile_and_run(
        r#"<?php
        function vars(): string {
            $a = 1;
            goto skip;
            $b = 2;
            skip:
            $c = 3;
            return implode(",", array_keys(get_defined_vars()));
        }
        echo vars();
        "#,
    );
    assert_eq!(out, "a,c");
}

/// Verifies an assignment a jump skips does not run, and one it does not skip does.
#[test]
fn test_goto_skipped_assignment() {
    let out = compile_and_run(
        r#"<?php
        function f(bool $jump): string {
            $x = null;
            $n = 1;
            if ($jump) goto skip;
            $x = "set";
            $n += 10;
            skip:
            $n += 100;
            return ($x ?? "null") . ":" . $n;
        }
        echo f(true), ",", f(false);
        "#,
    );
    assert_eq!(out, "null:101,set:111");
}

/// Verifies a local that holds different types on different jump paths.
#[test]
fn test_goto_value_changes_type_across_labels() {
    let out = compile_and_run(
        r#"<?php
        $x = 1;
        goto show;
        change:
        $x = "text";
        goto show;
        show:
        var_dump($x);
        if (is_int($x)) goto change;
        "#,
    );
    assert_eq!(out, "int(1)\nstring(4) \"text\"\n");
}

/// Verifies continue 2 written inside a wrapped inner loop body still targets the outer loop.
#[test]
fn test_goto_user_continue_two_through_dispatch() {
    let out = compile_and_run(
        r#"<?php
        for ($i = 0; $i < 3; $i++) {
            for ($j = 0; $j < 3; $j++) {
                $k = 0;
                again:
                $k++;
                if ($j === 1) continue 2;
                if ($k < 2) goto again;
                echo "$i$j ";
            }
        }
        echo "end";
        "#,
    );
    assert_eq!(out, "00 10 20 end");
}

/// Verifies a static declaration between a label and its jump is initialized once.
#[test]
fn test_goto_static_variable_in_dispatch_loop() {
    let out = compile_and_run(
        r#"<?php
        function tick(): int {
            $rounds = 0;
            again:
            static $calls = 0;
            $calls++;
            $rounds++;
            if ($rounds < 2) goto again;
            return $calls;
        }
        echo tick(), tick();
        "#,
    );
    assert_eq!(out, "24");
}

/// Verifies a literal eval fragment is a label scope of its own.
#[test]
fn test_goto_literal_eval_fragment() {
    let out = compile_and_run(
        r#"<?php
        $i = 0;
        eval('again: $i++; if ($i < 3) goto again;');
        again:
        echo $i;
        "#,
    );
    assert_eq!(out, "3");
}

/// Verifies a finally block that jumps internally does not disturb the jump that is leaving its try.
#[test]
fn test_goto_pending_jump_through_finally_with_its_own_jump() {
    let out = compile_and_run(
        r#"<?php
        function f(bool $flag): string {
            $out = "";
            try {
                $out .= "try ";
                goto after;
            } finally {
                if ($flag) goto tail;
                $out .= "f-body ";
                tail:
                $out .= "f-tail ";
            }
            $out .= "skipped ";
            after:
            return $out . "after";
        }
        echo f(true), "|", f(false);
        "#,
    );
    assert_eq!(out, "try f-tail after|try f-body f-tail after");
}

/// Verifies a jump lands two statements deep: an if branch inside a try body.
#[test]
fn test_goto_into_nested_if_inside_try() {
    let out = compile_and_run(
        r#"<?php
        goto deep;
        try {
            echo "no ";
            if ($argc > 100) {
                echo "no2 ";
                deep:
                echo "deep ";
            }
            echo "rest ";
        } finally {
            echo "fin";
        }
        "#,
    );
    assert_eq!(out, "deep rest fin");
}

/// Verifies a jump from one branch of an if to a label in its else branch.
#[test]
fn test_goto_between_if_branches() {
    let out = compile_and_run(
        r#"<?php
        function f(int $n): string {
            $out = "";
            if ($n > 0) {
                $out .= "then ";
                if ($n > 1) goto shared;
                $out .= "then-tail ";
            } else {
                $out .= "else ";
                shared:
                $out .= "shared ";
            }
            return $out . "end";
        }
        echo f(2), "|", f(1), "|", f(0);
        "#,
    );
    assert_eq!(out, "then shared end|then then-tail end|else shared end");
}

/// Verifies return and throw inside the wrapped range leave the function normally.
#[test]
fn test_goto_return_inside_dispatch_loop() {
    let out = compile_and_run(
        r#"<?php
        function find(array $items, int $needle): string {
            $i = 0;
            next:
            if ($i >= count($items)) return "missing";
            if ($items[$i] === $needle) return "at $i";
            $i++;
            goto next;
        }
        function fail(): void {
            $n = 0;
            again:
            $n++;
            if ($n === 3) throw new RuntimeException("gave up after $n");
            goto again;
        }
        echo find([4, 8, 15], 15), ",", find([4], 9), ",";
        try { fail(); } catch (RuntimeException $e) { echo $e->getMessage(); }
        "#,
    );
    assert_eq!(out, "at 2,missing,gave up after 3");
}

/// Verifies arrays and by-reference locals keep their values across a jump.
#[test]
fn test_goto_references_and_arrays_across_labels() {
    let out = compile_and_run(
        r#"<?php
        function collect(int $n): array {
            $items = [];
            $ref = &$items;
            more:
            $ref[] = count($items) * 2;
            if (count($items) < $n) goto more;
            return $items;
        }
        echo implode(",", collect(4));
        "#,
    );
    assert_eq!(out, "0,2,4,6");
}

/// Verifies goto stays usable as a method name, as in PHP.
#[test]
fn test_goto_goto_as_method_name() {
    let out = compile_and_run(
        r#"<?php
        class Nav {
            public function goto(string $where): string { return "to $where"; }
        }
        echo (new Nav())->goto("home");
        "#,
    );
    assert_eq!(out, "to home");
}

/// Verifies a fragment built at run time executes its jumps in the eval interpreter.
#[test]
fn test_goto_dynamic_eval_fragment() {
    let out = compile_and_run(
        r#"<?php
        $code = 'again: $i++; if ($i < ' . ($argc + 3) . ') goto again; echo "i=$i";';
        $i = 0;
        eval($code);
        "#,
    );
    assert_eq!(out, "i=4");
}

/// Verifies an included file is a label scope of its own: it may reuse the includer's label
/// names, and its jumps run each time the include executes inside the includer's own loop.
#[test]
fn test_goto_included_file_has_its_own_labels() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
                $n = 0;
                again:
                $n++;
                include 'part.php';
                if ($n < 2) goto again;
                echo "end";
                "#,
            ),
            (
                "part.php",
                r#"<?php
                $k = 0;
                again:
                $k++;
                if ($k === 1) goto skip;
                echo "body$n.$k ";
                skip:
                echo "tail$n.$k ";
                if ($k < 2) goto again;
                "#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "tail1.1 body1.2 tail1.2 tail2.1 body2.2 tail2.2 end");
}

/// Verifies an included file's `return` inside its jump loop ends the include, not the
/// includer, and still delivers the include's value.
#[test]
fn test_goto_included_file_returns_from_its_jump_loop() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
                $result = include 'lib.php';
                echo $result, "|after";
                "#,
            ),
            (
                "lib.php",
                r#"<?php
                $i = 0;
                loop:
                $i++;
                if ($i === 3) return "returned at $i";
                goto loop;
                "#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "returned at 3|after");
}
