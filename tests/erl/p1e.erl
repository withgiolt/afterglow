%% P1 unit tests: processes, links, monitors, exit signals, receive timeouts.
-module(p1e).
-export([t_basic/0, t_links/0, t_monitors/0, t_timeouts/0, t_selective/0, t_misc/0]).
-export([echo/1, crasher/1]).

echo(Parent) ->
    receive
        {ping, N} -> Parent ! {pong, N + 1}, echo(Parent);
        stop -> ok
    end.

crasher(Reason) ->
    receive go -> exit(Reason) end.

t_basic() ->
    Self = self(),
    P = spawn(?MODULE, echo, [Self]),
    P ! {ping, 1},
    A = receive {pong, X} -> X end,
    P2 = spawn(fun() -> Self ! {hello, self()} end),
    B = receive {hello, P2} -> got end,
    P ! stop,
    {A, B, is_pid(P), P =/= Self}.

t_links() ->
    process_flag(trap_exit, true),
    P = spawn_link(?MODULE, crasher, [boom]),
    P ! go,
    R1 = receive {'EXIT', P, Why} -> Why end,
    %% a linked process that is killed by a non-trapping link partner's death
    Self = self(),
    Mid = spawn_link(fun() ->
        C = spawn_link(?MODULE, crasher, [inner]),
        Self ! {child, C},
        receive never -> ok end
    end),
    C = receive {child, X} -> X end,
    C ! go,
    R2 = receive {'EXIT', Mid, W} -> W end,
    R3 = is_process_alive(C),
    %% normal exit of a linked process: an 'EXIT' message only because we trap
    N = spawn_link(fun() -> ok end),
    R4 = receive {'EXIT', N, W2} -> W2 end,
    %% exit/2 with kill
    K = spawn_link(fun() -> receive never -> ok end end),
    exit(K, kill),
    R5 = receive {'EXIT', K, W3} -> W3 end,
    %% exit/2 normal is ignored by a non-trapping process
    L = spawn(fun() -> receive stop -> Self ! stopped end end),
    exit(L, normal),
    L ! stop,
    R6 = receive stopped -> stopped end,
    process_flag(trap_exit, false),
    {R1, R2, R3, R4, R5, R6}.

t_monitors() ->
    P = spawn(?MODULE, crasher, [bye]),
    Ref = monitor(process, P),
    P ! go,
    R1 = receive {'DOWN', Ref, process, P, Why} -> Why end,
    %% monitor of a dead process: noproc
    Ref2 = monitor(process, P),
    R2 = receive {'DOWN', Ref2, process, P, W2} -> W2 end,
    %% demonitor with flush removes the DOWN message
    Q = spawn(fun() -> ok end),
    Ref3 = monitor(process, Q),
    receive after 10 -> ok end,
    true = demonitor(Ref3, [flush]),
    R3 = receive {'DOWN', Ref3, _, _, _} -> still_there after 0 -> flushed end,
    {S, Ref4} = spawn_monitor(fun() -> exit(normal) end),
    R4 = receive {'DOWN', Ref4, process, S, W4} -> W4 end,
    {R1, R2, R3, R4}.

t_timeouts() ->
    A = receive nothing -> x after 0 -> zero end,
    B = receive nothing -> x after 20 -> twenty end,
    Self = self(),
    spawn(fun() -> receive after 5 -> Self ! late end end),
    C = receive late -> late after 1000 -> missed end,
    D = try receive x -> x after -1 -> y end catch error:E -> {caught, E} end,
    {A, B, C, D}.

t_selective() ->
    Self = self(),
    [Self ! M || M <- [a, {b, 1}, c, {b, 2}, d]],
    X1 = receive {b, N} -> N end,
    X2 = receive c -> c end,
    X3 = receive {b, N2} -> N2 end,
    Rest = flush([]),
    {X1, X2, X3, Rest}.

flush(Acc) ->
    receive M -> flush([M | Acc]) after 0 -> lists:reverse(Acc) end.

t_misc() ->
    Self = self(),
    P = spawn(fun() -> receive stop -> ok end end),
    true = register(worker, P),
    W = whereis(worker),
    worker ! stop,
    receive after 10 -> ok end,
    Gone = whereis(worker),
    Alive = is_process_alive(P),
    self() ! one, self() ! two,
    {message_queue_len, QL} = process_info(self(), message_queue_len),
    _ = flush([]),
    Bad = try nobody_here ! hi catch error:badarg -> badarg end,
    {W =:= P, Gone, Alive, QL, Bad, Self =:= self()}.

