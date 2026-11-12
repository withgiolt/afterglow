//// The compute workloads bench/compute.sh runs on erl, afterglow and Gleam's
//// JavaScript target. Each one takes no arguments, runs once and returns an
//// Int, which the script prints as the workload's result line and compares
//// between erl and afterglow. Building a workload's input is part of what it
//// measures, so each input is small next to the work done on it.

import gleam/dict
import gleam/dynamic/decode
import gleam/int
import gleam/json
import gleam/list
import gleam/string
import gleam/string_tree

const fib_n = 27

const reverse_length = 10_000

const reverse_rounds = 100

const sort_length = 20_000

const json_records = 1000

const tree_pieces = 20_000

const dict_size = 10_000

const factorial_n = 1000

const bit_array_words = 32_768

// Keeps every checksum, and every product computed on the way, below 2^53, so
// JavaScript's float Ints compute the same values as the BEAM's integers.
const modulus = 1_000_000_007

/// Each workload's name, the name of its function here, and its input.
pub fn inputs() -> List(#(String, String)) {
  [
    #("fib", "fib(" <> int.to_string(fib_n) <> "), naive recursion"),
    #(
      "list_reverse",
      int.to_string(reverse_rounds)
        <> " reversals of a "
        <> int.to_string(reverse_length)
        <> "-element list",
    ),
    #(
      "list_sort",
      int.to_string(sort_length) <> " pseudo-random Ints, list.sort",
    ),
    #(
      "json",
      int.to_string(json_records)
        <> " records encoded with gleam_json, then parsed and decoded",
    ),
    #(
      "string_tree",
      int.to_string(tree_pieces)
        <> " numbers appended to a string_tree, then to_string",
    ),
    #(
      "dict",
      int.to_string(dict_size)
        <> " inserts into a Dict, then a lookup of every key",
    ),
    #(
      "bignum",
      int.to_string(factorial_n) <> "!, then reduced modulo 1,000,000,007",
    ),
    #(
      "bit_array",
      int.to_string(bit_array_words)
        <> " 16-bit values appended to a BitArray, then matched back out",
    ),
  ]
}

pub fn fib() -> Int {
  naive_fib(fib_n)
}

fn naive_fib(n: Int) -> Int {
  case n < 2 {
    True -> n
    False -> naive_fib(n - 1) + naive_fib(n - 2)
  }
}

pub fn list_reverse() -> Int {
  int.range(0, reverse_rounds, count_up(reverse_length), fn(items, _) {
    list.reverse(items)
  })
  |> checksum
}

pub fn list_sort() -> Int {
  pseudo_random(sort_length)
  |> list.sort(int.compare)
  |> checksum
}

type Record {
  Record(id: Int, name: String, score: Float, tags: List(String))
}

pub fn json() -> Int {
  let encoded =
    count_up(json_records)
    |> list.map(fn(i) {
      Record(
        id: i,
        name: "user-" <> int.to_string(i),
        score: int.to_float(i) /. 8.0,
        tags: ["gleam", int.to_string(i % 7)],
      )
    })
    |> json.array(encode_record)
    |> json.to_string
  case json.parse(encoded, decode.list(record_decoder())) {
    Ok(records) ->
      string.byte_size(encoded)
      + list.fold(records, 0, fn(sum, record) { sum + record.id })
    Error(_) -> -1
  }
}

fn encode_record(record: Record) -> json.Json {
  json.object([
    #("id", json.int(record.id)),
    #("name", json.string(record.name)),
    #("score", json.float(record.score)),
    #("tags", json.array(record.tags, json.string)),
  ])
}

fn record_decoder() -> decode.Decoder(Record) {
  use id <- decode.field("id", decode.int)
  use name <- decode.field("name", decode.string)
  use score <- decode.field("score", decode.float)
  use tags <- decode.field("tags", decode.list(decode.string))
  decode.success(Record(id:, name:, score:, tags:))
}

pub fn string_tree() -> Int {
  int.range(0, tree_pieces, string_tree.new(), fn(tree, i) {
    tree
    |> string_tree.append(int.to_string(i))
    |> string_tree.append(",")
  })
  |> string_tree.to_string
  |> string.byte_size
}

pub fn dict() -> Int {
  let table =
    int.range(0, dict_size, dict.new(), fn(table, key) {
      dict.insert(table, key, key * 2)
    })
  int.range(0, dict_size, 0, fn(sum, key) {
    case dict.get(table, key) {
      Ok(value) -> sum + value
      Error(_) -> sum
    }
  })
}

pub fn bignum() -> Int {
  int.range(1, factorial_n + 1, 1, fn(product, i) { product * i }) % modulus
}

pub fn bit_array() -> Int {
  int.range(0, bit_array_words, <<>>, fn(bits, i) { <<bits:bits, i:16>> })
  |> sum_words(0)
}

fn sum_words(bits: BitArray, sum: Int) -> Int {
  case bits {
    <<word:16, rest:bits>> -> sum_words(rest, { sum + word } % modulus)
    _ -> sum
  }
}

fn count_up(length: Int) -> List(Int) {
  int.range(length - 1, -1, [], fn(items, i) { [i, ..items] })
}

// x' = (75x + 74) mod 65537, so every product stays far below 2^53.
fn pseudo_random(count: Int) -> List(Int) {
  let #(_, items) =
    int.range(0, count, #(1, []), fn(state, _) {
      let #(x, items) = state
      let x = { x * 75 + 74 } % 65_537
      #(x, [x, ..items])
    })
  items
}

fn checksum(items: List(Int)) -> Int {
  list.fold(items, 0, fn(sum, x) { { sum * 31 + x } % modulus })
}
