<?php $source = $argc > 0 ? '
class DetectionListCow {
    public function __toString(): string {
        global $source_list;
        $source_list[1] = "bad";
        return "ASCII";
    }
}

$source_list = [new DetectionListCow(), "UTF-8"];
mb_detect_order($source_list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\\n"; }
echo $source_list[1], "\\n";
' : ''; eval($source);