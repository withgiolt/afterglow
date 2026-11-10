/// Translates between the request and response binaries index.js exchanges
/// with the runtime and wisp's Request/Response types. Apps call
/// `afterglow.serve`, not this module.
///
/// Both directions use Erlang's External Term Format rather than JSON:
/// `binary_to_term` and `term_to_binary` are runtime built-ins, while JSON
/// would be parsed and escaped by compiled Gleam code on every request.
import gleam/bit_array
import gleam/bytes_tree
import gleam/crypto
import gleam/http
import gleam/http/request
import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string
import wisp
import wisp/internal

pub fn serve(handler: fn(wisp.Request) -> wisp.Response) -> Nil {
  read_stdin()
  |> handle(handler)
  |> frame
  |> write_stdout
}

@external(erlang, "afterglow_io", "read_stdin")
fn read_stdin() -> BitArray

@external(erlang, "afterglow_io", "write_stdout")
fn write_stdout(data: BitArray) -> Nil

/// Ends the response frame, mirrored by SENTINEL in index.js. The frame is
/// the ETF payload, its byte length as a big-endian u32, then this marker.
/// Putting the marker last means index.js can take its last occurrence:
/// stray prints in user code all come before it, and a body that happens to
/// contain the marker sits before it too.
pub const sentinel = "\n__AFTERGLOW__"

fn frame(payload: BitArray) -> BitArray {
  let size = bit_array.byte_size(payload)
  bit_array.concat([payload, <<size:size(32)>>, bit_array.from_string(sentinel)])
}

/// Pure request/response bridge, without the stdin/stdout IO — this is
/// what tests drive directly. `request` is the ETF tuple
/// `{Method, Url, [{Name, Value}], Body}` index.js sends, all binaries, or
/// `{warmup, Method, Url, [{Name, Value}], Body}` for the warm-up; the
/// result is the ETF tuple `{Status, [{Name, Value}], Body}`.
pub fn handle(
  request: BitArray,
  handler: fn(wisp.Request) -> wisp.Response,
) -> BitArray {
  case decode_request(request) {
    Ok(#(Serve, req)) -> handler(req) |> encode_response
    Ok(#(Warmup, _)) -> wisp.no_content() |> encode_response
    Error(message) -> encode_error(400, message)
  }
}

/// index.js runs one throwaway request at startup so V8 compiles the wasm
/// before the first real request. That request is tagged in the payload
/// itself, which a client or binding can't forge, and goes through the
/// whole bridge except the user's handler.
type Purpose {
  Serve
  Warmup
}

fn decode_request(
  payload: BitArray,
) -> Result(#(Purpose, wisp.Request), String) {
  use #(purpose, method, url, headers, body) <- result.try(
    decode_request_term(payload)
    |> result.replace_error("invalid request payload"),
  )
  use method <- result.try(
    http.parse_method(method) |> result.replace_error("bad method"),
  )

  let #(path, query) = case string.split_once(url, "?") {
    Ok(#(path, query)) -> #(path, Some(query))
    Error(Nil) -> #(url, None)
  }
  let headers = list.map(headers, fn(h) { #(ascii_lowercase(h.0), h.1) })
  let host = list.key_find(headers, "host") |> result.unwrap("localhost")

  // Workers forbid generating random values at startup, where the warm-up
  // runs, and its request is never handled.
  let secret = case purpose {
    Serve -> crypto.strong_random_bytes(64) |> base64_encode
    Warmup -> ""
  }
  let connection = make_connection(body, secret)

  Ok(#(
    purpose,
    request.Request(
      method:,
      headers:,
      body: connection,
      scheme: http.Https,
      host:,
      port: None,
      path:,
      query:,
    ),
  ))
}

@external(erlang, "afterglow_bridge_ffi", "decode_request")
fn decode_request_term(
  payload: BitArray,
) -> Result(#(Purpose, String, String, List(#(String, String)), BitArray), Nil)

@external(erlang, "erlang", "term_to_binary")
fn term_to_binary(term: a) -> BitArray

/// `wisp.create_canned_connection` probes the file system for a temp dir
/// (`/tmp`, `/var/tmp`, `/usr/tmp`) to stream multipart uploads to. A
/// Worker has no file system, so `temporary_directory` is never read and a
/// fixed literal is correct.
fn make_connection(body: BitArray, secret: String) -> wisp.Connection {
  internal.Connection(
    reader: canned_reader(body),
    max_body_size: 8_000_000,
    max_files_size: 32_000_000,
    read_chunk_size: 1_000_000,
    secret_key_base: secret,
    temporary_directory: "./tmp/afterglow",
  )
}

fn canned_reader(data: BitArray) -> internal.Reader {
  fn(chunk_size) {
    case bit_array.byte_size(data) {
      0 -> Ok(internal.ReadingFinished)
      size -> {
        let take = int.min(chunk_size, size)
        let assert Ok(chunk) = bit_array.slice(data, 0, take)
        let assert Ok(rest) = bit_array.slice(data, take, size - take)
        Ok(internal.Chunk(chunk, canned_reader(rest)))
      }
    }
  }
}

fn encode_response(resp: wisp.Response) -> BitArray {
  case resp.body {
    wisp.File(..) ->
      encode_error(501, "file responses are not supported on afterglow")
    wisp.Text(text) ->
      term_to_binary(#(resp.status, resp.headers, bit_array.from_string(text)))
    wisp.Bytes(tree) ->
      term_to_binary(#(resp.status, resp.headers, bytes_tree.to_bit_array(tree)))
  }
}

fn encode_error(status: Int, message: String) -> BitArray {
  term_to_binary(#(
    status,
    [#("content-type", "text/plain; charset=utf-8")],
    bit_array.from_string(message),
  ))
}

@external(erlang, "base64", "encode")
fn base64_encode(bits: BitArray) -> String

@external(erlang, "erlang", "binary_to_list")
fn binary_to_list(s: String) -> List(Int)

@external(erlang, "erlang", "list_to_binary")
fn list_to_binary(l: List(Int)) -> String

/// HTTP header names are always ASCII tokens, so Unicode-aware lowercasing
/// isn't needed here.
fn ascii_lowercase(s: String) -> String {
  s
  |> binary_to_list
  |> list.map(fn(c) {
    case c >= 65 && c <= 90 {
      True -> c + 32
      False -> c
    }
  })
  |> list_to_binary
}
