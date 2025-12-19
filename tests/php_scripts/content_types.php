<?php

$type = $_GET['type'] ?? 'json';

switch ($type) {
    case 'json':
        header('Content-Type: application/json');
        echo json_encode(['format' => 'json', 'data' => [1, 2, 3]]);
        break;

    case 'xml':
        header('Content-Type: application/xml');
        echo '<?xml version="1.0"?><root><format>xml</format></root>';
        break;

    case 'html':
        header('Content-Type: text/html; charset=utf-8');
        echo '<!DOCTYPE html><html><body><p>HTML content</p></body></html>';
        break;

    case 'plain':
        header('Content-Type: text/plain');
        echo 'Plain text content';
        break;

    case 'css':
        header('Content-Type: text/css');
        echo 'body { color: red; }';
        break;

    case 'javascript':
        header('Content-Type: application/javascript');
        echo 'console.log("js");';
        break;

    case 'form':
        header('Content-Type: application/x-www-form-urlencoded');
        echo 'key1=value1&key2=value2';
        break;

    case 'multipart':
        $boundary = 'boundary123';
        header('Content-Type: multipart/mixed; boundary=' . $boundary);
        echo "--$boundary\r\n";
        echo "Content-Type: text/plain\r\n\r\n";
        echo "Part 1\r\n";
        echo "--$boundary--\r\n";
        break;

    case 'none':
        echo 'No content type set';
        break;

    case 'empty':
        header('Content-Type: application/json');
        break;

    default:
        header('Content-Type: ' . $type);
        echo 'Custom content type: ' . $type;
}
