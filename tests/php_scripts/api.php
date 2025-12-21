<?php
header('Content-Type: application/json');

class Person implements JsonSerializable
{
    private int $age;
    private string $name;
    private DateTimeInterface $birthdate;

    public function __construct(string $name, int $age, DateTimeInterface $birthdate)
    {
        $this->age = $age;
        $this->name = $name;
        $this->birthdate = $birthdate;
    }

    public function jsonSerialize(): mixed
    {
        return [
            'age' => $this->age,
            'name' => $this->name,
            'birthdate' => $this->birthdate->format('Y-m-d'),
            'greeting' => "Hello, my name is {$this->name} and I am {$this->age} years old.",
        ];
    }
}


$people = [
    new Person("Alice", 30, new DateTimeImmutable('1993-04-15')),
    new Person("Bob", 25, new DateTimeImmutable('1998-07-22')),
    new Person("Charlie", 35, new DateTimeImmutable('1988-01-10')),
    new Person("Dave", 40, new DateTimeImmutable('1983-11-05')),
    new Person("Eve", 28, new DateTimeImmutable('1995-09-30')),
    new Person("Frank", 33, new DateTimeImmutable('1990-06-12')),
    new Person("Grace", 29, new DateTimeImmutable('1994-03-18')),
    new Person("Hank", 31, new DateTimeImmutable('1992-08-25')),
    new Person("Ivy", 27, new DateTimeImmutable('1996-12-05')),
    new Person("Jack", 34, new DateTimeImmutable('1989-07-14')),
];

echo json_encode([
    'data' => [
        'message' => 'Hello from PHP!',
        'datetime' => (new DateTimeImmutable())->format('Y-m-d H:i:s'),
        'people' => $people,
    ]
], JSON_PRETTY_PRINT);