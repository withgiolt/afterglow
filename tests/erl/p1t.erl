%% Timers in tier 1 (virtual time), compared with erl.
-module(p1t).
-export([t_timers/0]).

t_timers() ->
    Self = self(),
    erlang:send_after(30, Self, late),
    erlang:send_after(10, Self, early),
    R = erlang:start_timer(20, Self, mid),
    C = erlang:send_after(15, Self, cancelled),
    Left = erlang:cancel_timer(C),
    Got = [receive M -> norm(M, R) end || _ <- [1, 2, 3]],
    Gone = erlang:cancel_timer(R),
    Late = receive cancelled -> yes after 50 -> no end,
    {is_integer(Left) andalso Left =< 15, Got, Gone, Late}.

norm({timeout, R, M}, R) -> {timeout, M};
norm(M, _) -> M.
