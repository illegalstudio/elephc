<?php

function remove_ref_key(array &$values, mixed $key): void
{
    unset($values[$key]);
}

$assoc = ["keep" => 1, "drop" => 2];
$copy = $assoc;
remove_ref_key($assoc, "drop");
echo count($assoc), ":", isset($assoc["drop"]) ? "bad" : "assoc", ":", count($copy), "|";
$list = [10, 20, 30];
remove_ref_key($list, 1);
echo count($list), ":", array_is_list($list) ? "list" : "assoc", ":", $list[2], "\n";
