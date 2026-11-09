//// P1 tests: processes inside one request. Run with tools/difftest.sh against erl.

import gleam/erlang/process.{type Subject}
import gleam/otp/actor

pub type Msg {
  Add(Int)
  Get(Subject(Int))
}

fn handle(state: Int, msg: Msg) -> actor.Next(Int, Msg) {
  case msg {
    Add(n) -> actor.continue(state + n)
    Get(reply) -> {
      process.send(reply, state)
      actor.continue(state)
    }
  }
}

/// A gleam_otp actor answering process.call.
pub fn actor_test() -> Int {
  let assert Ok(started) =
    actor.new(0) |> actor.on_message(handle) |> actor.start
  let s = started.data
  process.send(s, Add(5))
  process.send(s, Add(37))
  actor.call(s, 1000, Get)
}

/// 100,000 ping-pong messages between two processes (50,000 round trips).
pub fn pingpong_test() -> Int {
  let parent = process.new_subject()
  process.spawn(fn() {
    let inbox = process.new_subject()
    process.send(parent, inbox)
    pong(inbox)
  })
  let assert Ok(ponger) = process.receive(parent, 1000)
  ping(ponger, process.new_subject(), 50_000, 0)
}

fn pong(inbox: Subject(#(Int, Subject(Int)))) -> Nil {
  case process.receive_forever(inbox) {
    #(-1, _) -> Nil
    #(n, reply) -> {
      process.send(reply, n + 1)
      pong(inbox)
    }
  }
}

fn ping(ponger: Subject(#(Int, Subject(Int))), me: Subject(Int), n: Int, acc: Int) -> Int {
  case n {
    0 -> {
      process.send(ponger, #(-1, me))
      acc
    }
    _ -> {
      process.send(ponger, #(n, me))
      let assert Ok(v) = process.receive(me, 1000)
      ping(ponger, me, n - 1, acc + v)
    }
  }
}
