%% Sends to destinations with no process: dropped or badarg, like erl.
-module(sendx).
-export([t_dest/0]).

try_(F) -> try F() of V -> {ok, V} catch C:R -> {C, R} end.

t_dest() ->
    register(sendx_live, self()),
    Dead = spawn(fun() -> ok end),
    Alias = alias(), unalias(Alias),
    Dests = [nosuch, {nosuch, node()}, {sendx_live, node()}, sendx_live, {nosuch, 1},
             {sendx_live, 1}, {1, node()}, Dead, Alias, 42],
    R = [{try_(fun() -> D ! hi end), try_(fun() -> erlang:send(D, hi, []) end)} || D <- Dests],
    Got = [receive hi -> got after 0 -> none end || _ <- lists:seq(1, 6)],
    {R, Got}.
