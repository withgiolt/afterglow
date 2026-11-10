import afterglow
import wisp

/// 204 must reach the client without a body: Response rejects one.
pub fn start() {
  afterglow.serve(fn(_req) { wisp.no_content() })
}
