-module(bench_compute_erl).

%% The erl side of bench/compute.sh: times compute:WORKLOAD/0 in this VM with
%% timer:tc, after WARMUP untimed runs, and prints one JSON line in the shape
%% bench/compute/harness.mjs prints.

-export([main/1, inputs/0]).

main([Workload, WarmupStr, ItersStr]) ->
    Fun = list_to_atom(Workload),
    Warmup = list_to_integer(WarmupStr),
    Iters = list_to_integer(ItersStr),
    _ = [compute:Fun() || _ <- lists:seq(1, Warmup)],
    Runs = [timer:tc(compute, Fun, [], nanosecond) || _ <- lists:seq(1, Iters)],
    {_, Result} = lists:last(Runs),
    Ms = lists:sort([Ns / 1.0e6 || {Ns, _} <- Runs]),
    io:format("{\"result\":\"~b\",\"median_ms\":~.4f,\"p95_ms\":~.4f}~n",
              [Result, median(Ms), p95(Ms)]).

%% One "Workload<TAB>Input" line per workload, in the order compute:inputs/0 gives.
inputs() ->
    [io:format("~s\t~s~n", [Name, Input]) || {Name, Input} <- compute:inputs()],
    ok.

%% The same definitions as harness.mjs: the middle element (the upper one for
%% an even count) and the smallest value at least 95% of the runs don't exceed.
median(Sorted) -> lists:nth(length(Sorted) div 2 + 1, Sorted).
p95(Sorted) -> lists:nth(ceil(length(Sorted) * 0.95), Sorted).
