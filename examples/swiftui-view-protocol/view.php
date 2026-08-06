<?php

// Lot 2 of IOS_TARGET_SPEC.md: PHP describes a UI, a native host renders it.
//
// Nothing here draws anything. `render_view()` returns a serialized view tree
// and `dispatch()` returns the next one after an event, so the host stays a dumb
// renderer and every decision -- layout, labels, state -- lives in compiled PHP.
//
// That shape is what makes AOT viable for UI at all: a template engine would
// need a PHP runtime on the device to evaluate itself, while a tree *generator*
// compiles once and ships as machine code. It is also the one place in the
// mobile story where being AOT costs nothing.

/// Escapes the characters JSON forbids raw inside a string literal.
function json_escape(string $value): string {
    $out = '';
    $i = 0;
    $len = strlen($value);
    while ($i < $len) {
        $ch = $value[$i];
        if ($ch === '"') {
            $out = $out . '\\"';
        } elseif ($ch === '\\') {
            $out = $out . '\\\\';
        } elseif ($ch === "\n") {
            $out = $out . '\\n';
        } else {
            $out = $out . $ch;
        }
        $i = $i + 1;
    }
    return $out;
}

function text_node(string $value, string $style): string {
    return '{"t":"text","v":"' . json_escape($value) . '","style":"' . $style . '"}';
}

function button_node(string $label, string $action): string {
    return '{"t":"button","label":"' . json_escape($label) . '","action":"' . $action . '"}';
}

function row_node(string $children): string {
    return '{"t":"hstack","children":[' . $children . ']}';
}

function column_node(string $children): string {
    return '{"t":"vstack","children":[' . $children . ']}';
}

/// The whole application state, owned by PHP and surviving across host calls
/// because a function static lives in the loaded library's own memory.
function counter(int $delta): int {
    static $value = 0;
    $value = $value + $delta;
    if ($value < 0) {
        $value = 0;
    }
    return $value;
}

function describe(int $count): string {
    if ($count === 0) {
        return 'nothing yet';
    }
    if ($count === 1) {
        return 'one item';
    }
    return $count . ' items';
}

function build_tree(int $count): string {
    $header = text_node('elephc → SwiftUI', 'title');
    $status = text_node(describe($count), 'body');
    $buttons = row_node(
        button_node('−', 'dec') . ',' . button_node('+', 'inc') . ',' . button_node('reset', 'reset')
    );
    $footer = text_node('rendered by compiled PHP, drawn by SwiftUI', 'caption');
    return column_node($header . ',' . $status . ',' . $buttons . ',' . $footer);
}

#[Export]
function render_view(): string {
    return build_tree(counter(0));
}

#[Export]
function dispatch(string $action): string {
    if ($action === 'inc') {
        counter(1);
    } elseif ($action === 'dec') {
        counter(-1);
    } elseif ($action === 'reset') {
        counter(-1000000);
    }
    return build_tree(counter(0));
}
