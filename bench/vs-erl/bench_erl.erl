-module(bench_erl).

%% The erl side of bench/vs-erl.sh: MODULE:start/0 of a built Gleam project on
%% the BEAM, with the request already in memory. Prints one JSON line and
%% writes the framed response to OUT so the script can compare it with
%% afterglow's.

-export([main/1]).

main([Module, ReqFile, OutFile, NStr]) ->
    %% Loaded explicitly, not left to code path order, so the project's own
    %% afterglow_io can never win.
    Here = filename:dirname(code:which(?MODULE)),
    {module, afterglow_io} = code:load_abs(filename:join(Here, "afterglow_io")),
    Mod = list_to_atom(Module),
    N = list_to_integer(NStr),
    {ok, Req} = file:read_file(ReqFile),
    persistent_term:put(bench_request, Req),
    {FirstUs, _} = timer:tc(fun() -> Mod:start() end),
    ok = file:write_file(OutFile, get(bench_response)),
    loop(Mod, N div 10),
    {Us, _} = timer:tc(fun() -> loop(Mod, N) end),
    io:format("{\"first_request_ms\":~.3f,\"per_request_us\":~.3f,\"memory_kib\":~b}~n",
              [FirstUs / 1000, Us / N, erlang:memory(total) div 1024]),
    halt().

loop(_, 0) -> ok;
loop(Mod, K) -> Mod:start(), loop(Mod, K - 1).
