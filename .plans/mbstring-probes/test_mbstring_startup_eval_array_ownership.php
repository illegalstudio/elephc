<?php
// Historical startup integration probe for eval-array ownership; current behavior needs qualification.
// The earlier leak also appeared without startup overrides; scalar state getters were balanced.
mb_strlen("");
$source = $argc > 0 ? '
function show_detect_order(): void {
    $order = mb_detect_order();
    if (is_array($order)) { echo implode(",", $order); }
}
show_detect_order();
' : '';
eval($source);
