//// M4 GC test: allocates about 500 MB of short-lived garbage in total, keeping
//// little alive. `/?kind=list` makes list cells (process heap), `/?kind=bin`
//// makes binaries, `/?kind=depth&n=N` recurses N deep without tail calls.

import gleam/bit_array
import gleam/int
import gleam/list
import gleam/string
import gleam/option
import gleam/result
import gleam/uri
import afterglow
import wisp

pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(req: wisp.Request) -> wisp.Response {
  let query = case req.query {
    option.Some(q) -> uri.parse_query(q) |> result.unwrap([])
    option.None -> []
  }
  let get = fn(k, d) { list.key_find(query, k) |> result.unwrap(d) }
  let out = case get("kind", "list") {
    "bin" -> int.to_string(churn_bin(20_000, 0))
    "depth" -> {
      let n = int.parse(get("n", "1000")) |> result.unwrap(1000)
      int.to_string(depth(n))
    }
    _ -> int.to_string(churn_list(32_000, 0))
  }
  wisp.ok() |> wisp.string_body(out)
}

// 32,000 lists of 1,000 cells of 16 bytes = 512 MB of list garbage.
fn churn_list(n: Int, acc: Int) -> Int {
  case n {
    0 -> acc
    _ -> {
      let l = build(1000, [])
      churn_list(n - 1, acc + list.length(l))
    }
  }
}

// 20,000 binaries of about 25 KB = 500 MB of binary garbage.
fn churn_bin(n: Int, acc: Int) -> Int {
  case n {
    0 -> acc
    _ -> {
      let b = string.repeat("x", 25_000) |> bit_array.from_string
      churn_bin(n - 1, acc + bit_array.byte_size(b))
    }
  }
}

fn build(n: Int, acc: List(Int)) -> List(Int) {
  case n {
    0 -> acc
    _ -> build(n - 1, [n, ..acc])
  }
}

fn depth(n: Int) -> Int {
  case n {
    0 -> 0
    _ -> 1 + depth(n - 1)
  }
}
