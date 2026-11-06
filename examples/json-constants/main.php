<?php

// Keep Unicode paragraph separators literal in an exported text payload.
$text = "First paragraph\u{2029}Second paragraph";
$flags = JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_LINE_TERMINATORS;
echo json_encode(["text" => $text], $flags) . "\n";

// Unit enum cases need an explicit representation before JSON export.
enum ExportState {
    case Pending;
}

$result = json_encode(ExportState::Pending);
if ($result === false && json_last_error() === JSON_ERROR_NON_BACKED_ENUM) {
    echo "Export failed: " . json_last_error_msg() . "\n";
}

// A backed enum provides a stable scalar wire value for the public API.
enum ExportWireState: string {
    case Pending = "pending";
}
echo json_encode(["state" => ExportWireState::Pending]) . "\n";

// Custom serialization overrides the default for either enum kind.
enum ExportMessage implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return "ready"; }
}
echo json_encode(ExportMessage::Ready) . "\n";

// Return the case itself to export both its name and its stable backing value.
enum ExportCaseDetails: string implements JsonSerializable {
    case Ready = "ready";
    public function jsonSerialize(): mixed { return $this; }
}
echo json_encode(ExportCaseDetails::Ready) . "\n";
