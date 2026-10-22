<?php
// Reflection and callable invocation evaluate the same unpacked parameter default.
function pipelineSteps(array $steps = ['load', ...['validate', 'save']]): array {
    return $steps;
}

$parameter = new ReflectionParameter('pipelineSteps', 0);
echo json_encode($parameter->getDefaultValue()), "\n";

$pipeline = pipelineSteps(...);
echo json_encode($pipeline()), "\n";
