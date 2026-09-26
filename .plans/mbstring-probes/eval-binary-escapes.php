<?php
// PHP prints e9; the current eval lexer expands the escaped byte into UTF-8 and prints c3a9.
$source = $argc > 0 ? 'echo bin2hex("\xe9"), "\n";' : '';
eval($source);
