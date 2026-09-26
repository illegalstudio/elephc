<?php
// The startup integration exposed existing ownership debt in this eval array path.
// The same leak occurs without startup overrides; scalar state getters are balanced.
mb_strlen("");
$source = $argc > 0 ? '
function show_detect_order(): void {
    $order = mb_detect_order();
    if (is_array($order)) { echo implode(",", $order); }
}
show_detect_order();
' : '';
eval($source);
