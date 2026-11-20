import afterglow
import envie
import fixture_check
import wisp

pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(_req: wisp.Request) -> wisp.Response {
  let name = "AFTERGLOW_FIXTURE_ENV"
  let initial = envie.get(name)
  envie.set(name, "first")
  let first = envie.get(name)
  envie.set(name, "second")
  let second = envie.get(name)
  envie.unset(name)
  let missing = envie.get(name)
  envie.set(name, "until next request")

  fixture_check.run([
    #("initially absent", initial == Error(Nil)),
    #("put then get", first == Ok("first")),
    #("overwrite", second == Ok("second")),
    #("unset then get", missing == Error(Nil)),
  ])
}
