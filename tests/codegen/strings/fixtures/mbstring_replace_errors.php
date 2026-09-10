<?php
mb_ereg_search_getpos();
foreach (["mb_ereg_replace", "mb_eregi_replace"] as $name) {
    foreach ([[], ["a", "X"], ["a", "X", "a", null, "extra"], [[], "X", "a"], ["a", [], "a"],
              ["a", "X", []], ["a", "X", "a", []], ["[", "X", "a"], ["a", "X", "a", "Q"],
              ["[", "X", chr(255), "Q"], ["a", null, "a"], ["a", 42, "a"],
              ["", "X", ""], ["a", "X", "A", ""], ["a", "X", "A", null]] as $args) {
        try { var_dump(call_user_func_array($name, $args)); }
        catch (Throwable $error) { echo get_class($error), ":", $error->getMessage(), "\n"; }
    }
    var_dump(call_user_func($name, "(a+)+$", "X", "aaaaaaaaaaaaaaaaaaaaaaa!"));
}
