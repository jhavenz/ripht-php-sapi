<?php

header('Content-Type: application/json');

$throw = ($_GET['throw'] ?? 'false') === 'true';

set_exception_handler(function (Throwable $e) {
    http_response_code(500);
    echo json_encode([
        'error' => true,
        'type' => get_class($e),
        'message' => $e->getMessage(),
        'code' => $e->getCode(),
        'file' => basename($e->getFile()),
        'line' => $e->getLine(),
    ], JSON_PRETTY_PRINT);
});

if ($throw) {
    throw new RuntimeException('Test exception message', 42);
}

echo json_encode([
    'error' => false,
    'message' => 'No exception thrown',
], JSON_PRETTY_PRINT);
