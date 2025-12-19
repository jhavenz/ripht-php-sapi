<?php

header('Content-Type: application/json');

$all_server_keys = [
    'REQUEST_METHOD', 'REQUEST_URI', 'QUERY_STRING', 'SCRIPT_NAME',
    'SCRIPT_FILENAME', 'DOCUMENT_ROOT', 'SERVER_NAME', 'SERVER_PORT',
    'SERVER_PROTOCOL', 'SERVER_SOFTWARE', 'GATEWAY_INTERFACE',
    'REMOTE_ADDR', 'REMOTE_PORT', 'SERVER_ADDR',
    'CONTENT_TYPE', 'CONTENT_LENGTH',
    'REQUEST_TIME', 'REQUEST_TIME_FLOAT',
    'HTTPS', 'REQUEST_SCHEME',
    'PHP_SELF', 'PATH_INFO', 'PATH_TRANSLATED',
];

$server = [];
foreach ($all_server_keys as $key) {
    $server[$key] = $_SERVER[$key] ?? null;
}

$http_headers = [];
foreach ($_SERVER as $key => $value) {
    if (str_starts_with($key, 'HTTP_')) {
        $http_headers[substr($key, 5)] = $value;
    }
}

echo json_encode([
    'SERVER' => $server,
    'HTTP_HEADERS' => $http_headers,
    'GET' => $_GET,
    'POST' => $_POST,
    'COOKIE' => $_COOKIE,
    'FILES' => array_map(fn($f) => [
        'name' => $f['name'],
        'type' => $f['type'],
        'size' => $f['size'],
        'error' => $f['error'],
    ], $_FILES),
    'REQUEST' => $_REQUEST,
    'php_input' => file_get_contents('php://input'),
    'php_input_length' => strlen(file_get_contents('php://input')),
], JSON_PRETTY_PRINT | JSON_PARTIAL_OUTPUT_ON_ERROR);
