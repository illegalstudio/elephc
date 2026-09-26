<?php

$next_encoding = "bad";
class DetectionReference {
    public function __toString(): string { global $next_encoding; $next_encoding = "UTF-8"; return "ASCII"; }
}
$list = [new DetectionReference(), "bad"];
$next_encoding =& $list[1];
mb_detect_order($list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
echo $next_encoding, "\n";
