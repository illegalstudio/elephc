<?php
// Reproduces the remaining ownership issue when composing an eval source with .=.
// The append-history regression uses one source literal to isolate array mutation.
$source = '$a = ["seed" => 1, 9 => 2]; unset($a[9]); $a[] = 3; echo $a[10], "\n";';
$source .= '$full = ["seed" => 1, 9223372036854775807 => 2]; try { $full[] = 3; } catch (Error $error) { echo $error->getMessage(), "\n"; } echo $full[9223372036854775807], "\n";';
if ($argc > 1) { $source .= " "; }
eval($source);
