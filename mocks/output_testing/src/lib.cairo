use orion_numbers::F64;

#[derive(Drop)]
struct Nested {
    a: u32,
    b: i32,
    c: felt252,
    d: ByteArray
}

#[derive(Drop)]
struct AnotherNested {
    a: u32,
    b: i64,
}

fn main() -> (
    u32, felt252, Span<i32>, Span<Nested>, ByteArray, AnotherNested, bool, F64, Span<F64>
) {
    (
        42,
        'Hello World',
        array![1, -2, 3].span(),
        array![
            Nested { a: 10, b: -20, c: 30, d: "ABCD" },
            Nested { a: 40, b: -50, c: -60, d: "ABCDEFGHIJKLMNOPQRSTUVWXYZ12345" }
        ]
            .span(),
        "Hello world, how are you doing today?",
        AnotherNested { a: 1, b: 2 },
        true,
        F64 { d: 2147483648 },
        array![F64 { d: 2147483648 }, F64 { d: 2147483648 }].span()
    )
}
