<?php

function render_extract_defaults(array $context): void
{
    $name = 'kept';
    extract($context, EXTR_SKIP);
    echo $name, ':', $value, '|';
    extract(['name' => 'overwritten']);
    echo $name, "\n";
}

render_extract_defaults(['name' => 'ignored', 'value' => 42]);
