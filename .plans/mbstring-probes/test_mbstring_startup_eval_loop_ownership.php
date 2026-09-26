<?php
// A plain eval for-loop independently retained 26 blocks / 1040 bytes in the probe.
// Keep this separate from the now-balanced function_exists argument regression.
$source = $argc > 0 ? 'for ($i = 0; $i < 8; $i++) { echo "yes"; }' : '';
eval($source);
