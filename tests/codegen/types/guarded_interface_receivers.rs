//! Purpose:
//! Regression coverage for checked interface receiver narrowing in native EIR.
//!
//! Called from:
//! - `tests/codegen/types.rs`.
//!
//! Key details:
//! - Runtime choices keep guards observable and exercise both receiver classes.

use super::*;

/// A negated guard with an early return exposes the concrete property on fallthrough.
#[test]
fn test_guarded_interface_receiver_concrete_property() {
    let source = r#"<?php
namespace Routing;
interface Endpoint { public function name(): string; }
final class EndpointReference implements Endpoint {
    public string $type = "reference";
    public function name(): string { return $this->type; }
}
final class Direct implements Endpoint {
    public function name(): string { return "direct"; }
}
function choose(bool $reference): Endpoint {
    if ($reference) { return new EndpointReference(); }
    return new Direct();
}
function resolve(Endpoint $endpoint): string {
    if (!($endpoint instanceof EndpointReference)) { return $endpoint->name(); }
    return $endpoint->type;
}
echo resolve(choose($argc > 1));
"#;
    let expected = ["direct", "reference"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["reference"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["reference"]])), expected);
}

/// A parent-interface parameter dispatches a child-interface method only inside its guard.
#[test]
fn test_guarded_interface_receiver_child_method() {
    let source = r#"<?php
interface Game { public function update(): void; }
interface InputGame extends Game { public function pollInput(): void; }
final class Demo implements InputGame {
    public function update(): void { echo "update\n"; }
    public function pollInput(): void { echo "input\n"; }
}

final class Plain implements Game {
    public function update(): void { echo "update\n"; }
}
function choose(bool $input): Game {
    if ($input) { return new Demo(); }
    return new Plain();
}
function frame(Game $game): void {
    if ($game instanceof InputGame) { $game->pollInput(); }
    $game->update();
}
frame(choose($argc > 1));
"#;
    let expected = ["update\n", "input\nupdate\n"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["input"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["input"]])), expected);
}

/// Reassignment and fallthrough joins cannot reuse a preceding child-interface receiver view.
#[test]
fn test_guarded_interface_receiver_reassignment_and_join() {
    let source = r#"<?php
interface Task { public function name(): string; }
interface OptionalTask extends Task { public function extra(): string; }
final class ExtraTask implements OptionalTask {
    public function name(): string { return "extra"; }
    public function extra(): string { return "hook"; }
}
final class BasicTask implements Task {
    public function name(): string { return "basic"; }
}
function choose(bool $extra): Task {
    if ($extra) { return new ExtraTask(); }
    return new BasicTask();
}
function describe(Task $task): string {
    $out = "";
    if ($task instanceof OptionalTask) {
        $out = $task->extra() . ":";
        $task = choose(false);
        $out .= $task->name() . ":";
    }
    return $out . $task->name();
}
echo describe(choose($argc > 1));
"#;
    let expected = ["basic", "hook:basic:basic"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["extra"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["extra"]])), expected);
}

/// The receiver fact reaches methods and closures using their own body scopes.
#[test]
fn test_guarded_interface_receiver_method_and_closure_scopes() {
    let source = r#"<?php
interface Item {}
final class Label implements Item { public string $text = "label"; }
final class EmptyItem implements Item {}
final class Renderer {
    public function render(Item $item): string {
        if ($item instanceof Label) { return $item->text; }
        return "empty";
    }
}
$render = function (Item $item): string {
    if (!($item instanceof Label)) { return "empty"; }
    return $item->text;
};
$renderer = new Renderer();
echo $renderer->render(new Label()), ":", $renderer->render(new EmptyItem()), ":";
echo $render(new Label()), ":", $render(new EmptyItem());
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "label:empty:label:empty", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullsafe postfix chains use the same checked concrete and child-interface receiver views.
#[test]
fn test_guarded_interface_receiver_nullsafe_chain() {
    let source = r#"<?php
interface Entry {}
interface ReadableEntry extends Entry { public function read(): string; }
final class LabelEntry implements ReadableEntry {
    public string $text = "label";
    public function read(): string { return "read"; }
}
final class EmptyEntry implements Entry {}
function choose(bool $label): Entry {
    if ($label) { return new LabelEntry(); }
    return new EmptyEntry();
}
function render(Entry $entry): void {
    if ($entry instanceof LabelEntry) { echo $entry?->text; }
    if ($entry instanceof ReadableEntry) { echo $entry?->read(); }
}
render(choose($argc > 1));
"#;
    let expected = ["", "labelread"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["label"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["label"]])), expected);
}

/// A cached non-null property result still records the concrete class of its interface receiver.
#[test]
fn test_guarded_interface_receiver_after_property_null_guard() {
    let source = r#"<?php
interface Root {}
final class Label implements Root { public ?string $text = null; }
function choose(bool $filled): Root {
    $label = new Label();
    if ($filled) { $label->text = "ok"; }
    return $label;
}
function inspect(Root $root): string {
    if (!($root instanceof Label)) { return "other"; }
    if ($root->text === null) { return "null"; }
    return $root->text;
}
echo inspect(choose($argc > 1));
"#;
    let expected = ["null", "ok"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["filled"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["filled"]])), expected);
}

/// Silent probes avoid uninitialized reads while an ordinary nullsafe read still raises Error.
#[test]
fn test_guarded_interface_receiver_uninitialized_property_probes() {
    let source = r#"<?php
interface Root {}
final class Label implements Root { public int $text; }
function choose(bool $filled): Root {
    $label = new Label();
    if ($filled) { $label->text = 7; }
    return $label;
}
function inspect(Root $root): void {
    if (!($root instanceof Label)) { echo "other"; return; }
    echo isset($root->text) ? "set|" : "unset|";
    echo empty($root->text) ? "empty|" : "full|";
    echo $root->text ?? "plain", "|", $root?->text ?? "safe", "|";
    try { echo $root?->text; }
    catch (Error $error) { echo "error"; }
}
inspect(choose($argc > 1));
"#;
    let expected = ["unset|empty|plain|safe|error", "set|full|7|7|7"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["filled"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["filled"]])), expected);
}

/// Loop storage probes cannot erase guards for stored properties, methods and silent probes.
#[test]
fn test_guarded_interface_receiver_stored_loop_results() {
    let source = r#"<?php
interface Endpoint {}
interface Readable extends Endpoint { public function read(): int; }
final class Reference implements Readable {
    public int $value = 3;
    public int $pending;
    public function read(): int { return 4; }
}
final class Direct implements Endpoint {}
function choose(bool $reference): Endpoint {
    if ($reference) { return new Reference(); }
    return new Direct();
}
function inspect(Endpoint $endpoint): void {
    $sum = 0;
    $values = [];
    for ($i = 0; $i < 2; $i++) {
        if ($endpoint instanceof Reference) {
            $sum += $endpoint->value;
            $values[] = $endpoint->value;
            $set = isset($endpoint->pending);
            $empty = empty($endpoint->pending);
            $plain = $endpoint->pending ?? "p";
            $safe = $endpoint?->pending ?? "s";
            echo $set ? "set:" : "unset:", $empty ? "empty:" : "full:", $plain, ":", $safe, ";";
        }
        if ($endpoint instanceof Readable) {
            $method = $endpoint->read();
            echo $method, ";";
        }
    }
    $j = 0;
    while ($j < 1) {
        if ($endpoint instanceof Reference) { $value = $endpoint->value; echo $value, ";"; }
        $j++;
    }
    echo $sum, ":", count($values);
}
inspect(choose($argc > 1));
"#;
    let expected = ["0:0", "unset:empty:p:s;4;unset:empty:p:s;4;3;6:2"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["reference"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["reference"]])), expected);
}

/// Loop-created captures ignore speculative types while the real walk checks named callees.
#[test]
fn test_guarded_interface_receiver_loop_created_closures() {
    let source = r#"<?php
interface Endpoint {}
interface Readable extends Endpoint { public function read(): int; }
final class Reference implements Readable {
    public int $value = 7;
    public function read(): int { return 4; }
}
final class Direct implements Endpoint {}
function choose(bool $reference): Endpoint {
    if ($reference) { return new Reference(); }
    return new Direct();
}
function inspect(Endpoint $endpoint): void {
    for ($i = 0; $i < 1; $i++) {
        if ($endpoint instanceof Reference) {
            $read = function () use ($endpoint): int { return $endpoint->value; };
            echo $read(), ":";
        }
        $typed = typedRead($endpoint);
        $inferred = inferredRead($endpoint);
        echo $typed, $inferred, ":";
    }
    $j = 0;
    while ($j < 1) {
        if ($endpoint instanceof Readable) {
            $call = function () use ($endpoint): int { return $endpoint->read(); };
            echo $call(), ":";
        }
        $j++;
    }
    do {
        if ($endpoint instanceof Reference) {
            $arrow = fn(): int => $endpoint->value;
            echo $arrow(), ":";
        }
    } while (false);
    echo "end";
}
function typedRead(Endpoint $endpoint): int {
    if ($endpoint instanceof Reference) { return $endpoint->value; }
    return 0;
}
function inferredRead(Endpoint $endpoint) {
    if ($endpoint instanceof Reference) { return $endpoint->value; }
    return 0;
}
function userMain(Endpoint $endpoint): int {
    if ($endpoint instanceof Reference) { return $endpoint->value; }
    return 0;
}
inspect(choose($argc > 1));
for ($outer = 0; $outer < 1; $outer++) {
    $main = userMain(choose($argc > 1));
    echo ":", $main;
}
"#;
    let expected = ["00:end:0", "7:77:4:7:end:7"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["reference"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["reference"]])), expected);
}

/// Dynamic names use the guarded class for reads and silent uninitialized-property probes.
#[test]
fn test_guarded_interface_receiver_dynamic_property_names() {
    let source = r#"<?php
interface Endpoint {}
final class Reference implements Endpoint {
    public int $value = 7;
    public int $pending;
    public float $weight;
    public ?string $note;
}
final class Direct implements Endpoint {}
function choose(bool $reference): Endpoint {
    if ($reference) { return new Reference(); }
    return new Direct();
}
function inspect(Endpoint $endpoint, string $name, string $probe): void {
    if (!($endpoint instanceof Reference)) { echo "other"; return; }
    echo $endpoint->{'value'}, ":", $endpoint?->{'value'}, ":";
    echo $endpoint->$name, ":", $endpoint?->{$name}, ":";
    echo $endpoint->{'pending'} ?? "p", ":", $endpoint?->{'pending'} ?? "s", ":";
    echo $endpoint->$probe ?? "r", ":", $endpoint?->{$probe} ?? "n", ":";
    echo isset($endpoint->{$name}) ? "set:" : "unset:";
    echo empty($endpoint->{$probe}) ? "empty" : "full";
    echo ":", $endpoint->{'weight'} ?? "float", ":", $endpoint?->{'note'} ?? "string";
    try { echo $endpoint->{'pending'}; } catch (Error $error) { echo ":error"; }
}
inspect(choose($argc > 1), $argv[1] ?? "value", $argv[2] ?? "pending");
"#;
    let expected = ["other", "7:7:7:7:p:s:r:n:set:empty:float:string:error"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["value", "pending"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["value", "pending"]])), expected);
}

/// Buffer probes retain their element layout and still throw lazily for uninitialized slots.
#[test]
fn test_guarded_interface_receiver_dynamic_buffer_probes() {
    let source = r#"<?php
interface Endpoint {}
final class Buffered implements Endpoint { public buffer<int> $data; }
function choose(bool $initialized): Endpoint {
    $value = new Buffered();
    if ($initialized) {
        $value->data = buffer_new<int>(1);
        $value->data[0] = 9;
    }
    return $value;
}
function inspect(Endpoint $endpoint, string $name): void {
    if (!($endpoint instanceof Buffered)) { return; }
    try {
        $literal = $endpoint->{'data'} ?? throw new RuntimeException("literal");
        echo $literal[0];
    } catch (RuntimeException $error) { echo $error->getMessage(); }
    echo ":";
    try {
        $dynamic = $endpoint->$name ?? throw new RuntimeException("dynamic");
        echo $dynamic[0];
    } catch (RuntimeException $error) { echo $error->getMessage(); }
}
inspect(choose($argc > 1), $argv[1] ?? "data");
"#;
    let expected = ["literal:dynamic", "9:9", "9:dynamic"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["data"], &["absent"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["data"], &["absent"]])), expected);
    let heap_source = source.replace(
        "inspect(choose($argc > 1), $argv[1] ?? \"data\");",
        r#"function heapControl(): void {
    $value = new Buffered();
    $buffer = buffer_new<int>(1);
    $value->data = $buffer;
    $value->data[0] = 9;
    inspect($value, "data");
    inspect($value, "absent");
    buffer_free($buffer);
}
heapControl();"#,
    );
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(&heap_source));
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:99:dynamic", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
