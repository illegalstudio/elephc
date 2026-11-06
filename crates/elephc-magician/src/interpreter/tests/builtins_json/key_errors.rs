//! Purpose:
//! Checks JSON key failure traversal and pending errors across callback exceptions.
//!
//! Called from:
//! - The parent interpreter JSON builtin test module.
//!
//! Key details:
//! - Expected output is verified against PHP8.5.11.
//! - Throw-only last-error preservation is an existing separate compatibility issue.

use super::*;

/// Invalid UTF-8 keys continue values and siblings, unlike invalid string values.
#[test]
fn execute_program_json_invalid_keys_preserve_callbacks_and_error_priority() {
    let program = parse_fragment(br#"enum LaterKey implements JsonSerializable { case R; public function jsonSerialize():mixed {echo "later:";return 4;} }
enum ThrowKey implements JsonSerializable {case R;public function jsonSerialize():mixed {echo "throw:".json_last_error().":";throw new Exception("user");}}
$bad=chr(255);
foreach ([0,JSON_THROW_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags){
 json_encode(9);
 try{$r=json_encode([$bad=>1,"ok"=>LaterKey::R],$flags);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "json:".$e->getCode()."\n";}

 json_encode(9);
 $r=json_encode([$bad=>1,"a"=>INF],$flags & ~JSON_THROW_ON_ERROR);echo ($r===false?"false":$r).":".json_last_error()."\n";
 json_encode(9);
 try{$r=json_encode([[$bad=>1]],$flags,1);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "depth:".$e->getCode()."\n";}
 json_encode(9);
 try{$r=json_encode([$bad,LaterKey::R],$flags);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "value:".$e->getCode()."\n";}
}

foreach ([0,JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags) {
 json_encode(9);
 try {json_encode([$bad=>ThrowKey::R],$flags);} catch(Exception $e) {echo "user:".$e->getMessage().":".json_last_error()."\n";}
}
"#).expect("parse JSON failure callback fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("recover from JSON callback failures");
    assert_eq!(values.output, concat!(
        "later:false:5\n",
        "false:7\n",
        "false:1\n",
        "false:5\n",
        "later:json:5\n",
        "false:7\n",
        "depth:1\n",
        "value:5\n",
        "later:{\"\":1,\"ok\":4}:5\n",
        "{\"\":1,\"a\":0}:7\n",
        "[{\"\":1}]:1\n",
        "later:[null,4]:5\n",
        "later:{\"\":1,\"ok\":4}:5\n",
        "{\"\":1,\"a\":0}:7\n",
        "[{\"\":1}]:1\n",
        "later:[null,4]:5\n",
        "throw:0:user:user:5\n",
        "throw:0:user:user:5\n",
        "throw:0:user:user:5\n",
    ));
}

/// Pending partial depth and recursion errors become visible after a user exception.
#[test]
fn execute_program_json_partial_errors_survive_user_exceptions() {
    let program = parse_fragment(br#"
enum PartialFailure implements JsonSerializable {case R;public function jsonSerialize():mixed{return [[1]];}}
enum PartialCycle implements JsonSerializable {case R;public function jsonSerialize():mixed{return [$this];}}
enum PartialThrow implements JsonSerializable {
 case R;
 public function jsonSerialize():mixed {echo "inside:".json_last_error().":";throw new Exception("user");}
}
foreach ([JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags) {
 foreach ([1,512] as $limit) {
  json_encode(9);
  $first=$limit===1?PartialFailure::R:PartialCycle::R;
  try {json_encode([$first,PartialThrow::R],$flags,$limit);}
  catch(Exception $e) {echo $e->getMessage().":".json_last_error()."\n";}
 }
}
"#).expect("parse JSON failure callback fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("recover from JSON callback failures");
    assert_eq!(values.output, concat!(
        "inside:0:user:1\n",
        "inside:0:user:6\n",
        "inside:0:user:1\n",
        "inside:0:user:6\n",
    ));
}
