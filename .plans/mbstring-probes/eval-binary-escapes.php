<?php
// Historical discrepancy: PHP printed e9 while eval printed c3a9.
// The lexer now has raw-byte regression coverage; qualify this full eval source before retirement.
$source = $argc > 0 ? 'echo bin2hex("\xe9"), "\n";' : '';
eval($source);
