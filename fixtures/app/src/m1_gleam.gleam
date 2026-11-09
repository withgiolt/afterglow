//// M1 tests in Gleam: fib(30) and summing 1..1_000_000 as a list.

import gleam/list

pub fn fib(n: Int) -> Int {
  case n {
    0 -> 0
    1 -> 1
    _ -> fib(n - 1) + fib(n - 2)
  }
}

pub fn fib_test() -> Int {
  fib(30)
}

fn build(n: Int, acc: List(Int)) -> List(Int) {
  case n {
    0 -> acc
    _ -> build(n - 1, [n, ..acc])
  }
}

pub fn sum_test() -> Int {
  build(1_000_000, []) |> list.fold(0, fn(acc, x) { acc + x })
}
