"""Capture ordinary mb_output_handler phase and codec behavior from PHP 8.5.10."""

import gzip
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
DESTINATION = ROOT / "crates/elephc-mbstring/tests/fixtures/output_handler.jsonl.gz"
WORKER = r'''
$case = json_decode(stream_get_contents(STDIN), true, 512, JSON_THROW_ON_ERROR);
mb_internal_encoding($case["from"]);
mb_http_output($case["to"]);
mb_substitute_character($case["substitute"]);
$source = @mb_convert_encoding(str_repeat("abc Café 猫 東京 한글 😀 123!? ", 12), $case["from"], "UTF-8");
$pieces = [substr($source, 0, 5), substr($source, 5, 125), substr($source, 130)];
$steps = [[0, "before"], [1, $pieces[0]], [0, $pieces[1]], [8, $pieces[2]], [0, "after"]];
if (isset($case["segments"])) {
    $steps = [];
    foreach ($case["segments"] as [$phase, $text]) {
        $steps[] = [$phase, @mb_convert_encoding($text, $case["from"], "UTF-8")];
    }
}
$result = [];
foreach ($steps as $index => [$phase, $input]) {
    foreach (($case["changes"][$index] ?? []) as $key => $value) {
        if ($key === "from") { mb_internal_encoding($value); }
        if ($key === "to") { mb_http_output($value); }
        if ($key === "substitute") { mb_substitute_character($value); }
    }
    $before = mb_get_info("illegal_chars");
    $output = mb_output_handler($input, $phase);
    $result[] = ["phase" => $phase, "input" => bin2hex($input), "output" => bin2hex($output),
        "errors" => mb_get_info("illegal_chars") - $before];
}
$case["steps"] = $result;
echo json_encode($case, JSON_THROW_ON_ERROR);
'''


def main() -> None:
    """Write reproducible compressed fixtures, isolating each codec sequence in a PHP request."""
    version = subprocess.check_output(["php", "-r", "echo PHP_VERSION;"], text=True)
    if version != "8.5.10":
        raise SystemExit(f"Expected PHP 8.5.10, found {version}")
    names = json.loads((ROOT / "scripts/mbstring/php_surface.json").read_text())["encoding_order"]
    cases = []
    for name in names:
        cases.extend([{"from": "UTF-8", "to": name, "substitute": 63},
                      {"from": name, "to": "UTF-8", "substitute": 63}])
    for substitute in ["none", "long", "entity", 0x2603]:
        for target in ["ASCII", "SJIS-mac", "SJIS-Mobile#DOCOMO", "ISO-2022-JP", "UTF-7"]:
            cases.append({"from": "UTF-8", "to": target, "substitute": substitute})
    segments = [[1, "1"], [0, "#"], [8, "🇯"], [1, "x🇯"], [8, "🇵"],
                [9, "1\u20e3"], [9, "\uf8600."], [1, "\uf861XIII"], [8, "abc"],
                [1, "e\u0301"], [8, ""]]
    for target in names:
        cases.append({"from": "UTF-8", "to": target, "substitute": 63, "segments": segments})
    for substitute in ["none", "long", "entity", 0x2603]:
        cases.append({"from": "UTF-8", "to": "SJIS-mac", "substitute": substitute, "segments": segments})
    cases.extend([
        {"from": "UTF-8", "to": "ISO-8859-1", "substitute": 63,
         "segments": [[1, "é"], [8, "é"], [0, "é"], [8, "é"], [0, "é"]],
         "changes": [{}, {"to": "pass"}, {"to": "ISO-8859-1"}, {}, {}]},
        {"from": "UTF-8", "to": "ASCII", "substitute": 63,
         "segments": [[1, "猫"], [0, "猫"], [8, "猫"], [1, "猫"], [8, ""]],
         "changes": [{}, {"substitute": "long"}, {"substitute": "none"}, {"substitute": 0x2603}, {}]},
        {"from": "UTF-8", "to": "UTF-8", "substitute": 63,
         "segments": [[1, "é"], [0, "é"], [8, "é"], [1, "é"], [8, "é"]],
         "changes": [{}, {"from": "ISO-8859-1"}, {"from": "UTF-8"}, {"to": "UTF-16LE"}, {}]},
    ])
    rows = []
    for case in cases:
        result = subprocess.run(["php", "-d", "display_errors=stderr", "-r", WORKER],
                                input=json.dumps(case), text=True, capture_output=True, timeout=15, check=True)
        if result.stderr:
            raise RuntimeError(f"Unexpected PHP diagnostic for {case}: {result.stderr}")
        rows.append(json.dumps(json.loads(result.stdout), ensure_ascii=True, separators=(",", ":")))
    DESTINATION.write_bytes(gzip.compress(("\n".join(rows) + "\n").encode(), mtime=0))
    print(f"Wrote {len(rows)} request sequences to {DESTINATION}")


if __name__ == "__main__":
    main()
