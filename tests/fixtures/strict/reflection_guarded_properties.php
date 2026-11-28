<?php
class GuardedReflectionSubject {}
class GuardedReflectionOwner extends ReflectionClass {
    public string $label = 'unlabeled';
    public function __construct(string $name, string $label) {
        parent::__construct($name);
        $this->label = $label;
    }
}
function guardedReflectionDefault(string $value = 'ok'): void {}
$owner = new GuardedReflectionOwner('GuardedReflectionSubject', 'tag');
$parameter = (new ReflectionFunction('guardedReflectionDefault'))->getParameters()[0];
echo $owner->getName(), ':', $owner->label, '|', $parameter->getDefaultValue(), '|';
class GuardedByValueBox {
    public string $value;
    public function __construct(string $value) { echo 'B'; $this->value = $value; }
}
function guardedByValueFactory(string $class, mixed $value): mixed { return new $class($value); }
$box = guardedByValueFactory('GuardedByValueBox', 'vv');
echo $box->value, '|';
try { $converted = guardedByValueFactory('GuardedByValueBox', 2); echo $converted->value, '|'; }
catch (TypeError $error) { echo 'K|'; unset($error); }
