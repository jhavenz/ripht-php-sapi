<?php
/**
 * Basic test script with JSON response.
 * 
 * Tests header setting, status code, and JSON output.
 */

header('Content-Type: application/json');
http_response_code(200);

$response = [
    'status' => http_response_code(),
    'output' => 'Hello from PHP',
    'headers_sent' => headers_sent(),
];

echo json_encode($response, JSON_PRETTY_PRINT);

