<?php
/**
 * HTTP headers test script.
 * 
 * Tests custom header setting and retrieval.
 */

header('Content-Type: application/json');
header('X-Custom-Header: test-value');
header('X-Another-Header: another-value');

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? 'UNKNOWN',
    'headers_sent' => headers_sent(),
    'response_code' => http_response_code(),
];

echo json_encode($response, JSON_PRETTY_PRINT);

