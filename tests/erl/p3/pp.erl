%% P3: messages between nodes (Durable Objects).
-module(pp).
-export([start_ponger/0, start_pinger/2, result/0, watch/1, watch_result/0, poke/1]).
-export([ponger/0, pinger/3, watcher/1]).

start_ponger() ->
    case whereis(ponger) of
        undefined -> register(ponger, spawn(?MODULE, ponger, [])), started;
        _ -> running
    end.

ponger() ->
    receive
        {ping, From, I} -> From ! {pong, I}, ponger();
        die_node -> glowc:node_crash(), ponger()
    end.

%% N round trips to {ponger, Node}; the result is kept by the pinger process.
start_pinger(Node, N) ->
    register(pinger, spawn(?MODULE, pinger, [Node, N, self()])),
    started.

pinger(Node, N, _) ->
    T0 = erlang:monotonic_time(microsecond),
    ok = loop(Node, 1, N),
    T1 = erlang:monotonic_time(microsecond),
    done(N, T1 - T0).

loop(_, I, N) when I > N -> ok;
loop(Node, I, N) ->
    {ponger, Node} ! {ping, self(), I},
    receive {pong, I} -> loop(Node, I + 1, N) end.

done(N, Us) ->
    receive {result, From} -> From ! {done, N, Us}, done(N, Us) end.

%% The pinger answers only when done; a timeout means it is still running.
result() ->
    case whereis(pinger) of
        undefined -> none;
        P -> P ! {result, self()},
             receive R -> R after 1 -> running end
    end.

watch(Node) ->
    register(watcher, spawn(?MODULE, watcher, [Node])),
    started.

watcher(Node) ->
    Ref = monitor(process, {ponger, Node}),
    receive {'DOWN', Ref, process, _, Why} -> keep(Why) end.

keep(Why) -> receive {result, From} -> From ! {down, Why}, keep(Why) end.

watch_result() ->
    whereis(watcher) ! {result, self()},
    receive R -> R after 1 -> watching end.

poke(Node) ->
    {ponger, Node} ! die_node,
    poked.
