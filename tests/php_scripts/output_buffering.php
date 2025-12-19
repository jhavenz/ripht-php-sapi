<?php

ob_start();
echo "First buffered content\n";
$first = ob_get_contents();

ob_start();
echo "Nested buffer content\n";
$nested = ob_get_clean();

echo "Second buffered content\n";
$second = ob_get_contents();
ob_end_clean();

ob_start(function ($buffer) {
    return strtoupper($buffer);
});
echo "transformed content\n";
$transformed = ob_get_clean();

header('Content-Type: application/json');
echo json_encode([
    'first' => $first,
    'nested' => $nested,
    'second' => $second,
    'transformed' => $transformed,
    'final_level' => ob_get_level(),
], JSON_PRETTY_PRINT);
