<?php
// Historical observation: this plain eval loop retained 26 blocks / 1040 bytes.
// No current failure or replay result is asserted here.
// Keep this separate from the now-balanced function_exists argument regression.
$source = $argc > 0 ? 'for ($i = 0; $i < 8; $i++) { echo "yes"; }' : '';
eval($source);
