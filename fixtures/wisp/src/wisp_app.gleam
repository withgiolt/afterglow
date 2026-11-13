import afterglow
import gleam/dynamic/decode
import gleam/http.{Get, Post}
import gleam/int
import gleam/json
import gleam/list
import gleam/result
import wisp.{type Request, type Response}

/// A wisp app with a router, a middleware, path parameters, query strings,
/// JSON and form bodies, plain cookies and redirects. The wisp fixture CI job
/// sends one request per route (tools/wisp-fixture.sh) and compares each
/// response with erl. Signed cookies are left out: the bridge draws a new
/// secret key base on every request, so their values differ between runs.
pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(req: Request) -> Response {
  use req <- wisp.handle_head(req)
  use <- add_header
  case wisp.path_segments(req), req.method {
    [], Get -> wisp.ok() |> wisp.string_body("hello from wisp")
    ["users", id], Get -> user(id)
    ["search"], Get -> search(req)
    ["json"], Post -> echo_json(req)
    ["form"], Post -> greet_form(req)
    ["visits"], Get -> visits(req)
    ["old"], Get -> wisp.redirect("/")
    ["json"], _ | ["form"], _ -> wisp.method_not_allowed([Post])
    _, _ -> wisp.not_found()
  }
}

/// A middleware: runs the rest of the handler, then adds a header to its response.
fn add_header(next: fn() -> Response) -> Response {
  next() |> wisp.set_header("x-fixture", "wisp")
}

fn user(id: String) -> Response {
  case int.parse(id) {
    Ok(n) -> wisp.ok() |> wisp.string_body("user " <> int.to_string(n))
    Error(Nil) -> wisp.bad_request("user id must be a number")
  }
}

fn search(req: Request) -> Response {
  let terms =
    wisp.get_query(req)
    |> list.filter_map(fn(pair) {
      case pair {
        #("q", term) -> Ok(term)
        _ -> Error(Nil)
      }
    })
  json.array(terms, json.string)
  |> json.to_string
  |> wisp.json_response(200)
}

fn echo_json(req: Request) -> Response {
  use body <- wisp.require_json(req)
  let decoder = {
    use name <- decode.field("name", decode.string)
    use age <- decode.field("age", decode.int)
    decode.success(#(name, age))
  }
  case decode.run(body, decoder) {
    Ok(#(name, age)) ->
      json.object([
        #("name", json.string(name)),
        #("age_next_year", json.int(age + 1)),
      ])
      |> json.to_string
      |> wisp.json_response(201)
    Error(_) -> wisp.unprocessable_content()
  }
}

fn greet_form(req: Request) -> Response {
  use form <- wisp.require_form(req)
  case list.key_find(form.values, "name") {
    Ok(name) -> wisp.ok() |> wisp.string_body("hello " <> name)
    Error(Nil) -> wisp.bad_request("missing name")
  }
}

fn visits(req: Request) -> Response {
  let seen =
    wisp.get_cookie(req, "visits", wisp.PlainText)
    |> result.try(int.parse)
    |> result.unwrap(0)
  wisp.ok()
  |> wisp.string_body("visit " <> int.to_string(seen + 1))
  |> wisp.set_cookie(
    req,
    "visits",
    int.to_string(seen + 1),
    wisp.PlainText,
    60 * 60,
  )
}
