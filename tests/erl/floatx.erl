%% Integers too large for a float: badarg or badarith like erl, never infinity.
-module(floatx).
-export([t_huge/0]).

try_(F) -> try F() of V -> {ok, V} catch C:R -> {C, R} end.

t_huge() ->
    Big = [1 bsl 1023, 1 bsl 1024, 1 bsl 2000, -(1 bsl 2000)],
    [try_(fun() -> float(N) end) || N <- Big]
    ++ [try_(fun() -> N / 2 end) || N <- Big]
    ++ [try_(fun() -> N * 1.5 end) || N <- Big]
    ++ [try_(fun() -> N + 0.5 end) || N <- Big]
    ++ [try_(fun() -> math:sqrt(N) end) || N <- Big]
    ++ [try_(fun() -> math:pow(N, 1) end) || N <- Big]
    ++ [try_(fun() -> math:pow(2, N) end) || N <- [x, 1 bsl 2000]] ++ [try_(fun() -> math:sqrt(x) end)]
    ++ [try_(fun() -> <<N:64/float>> end) || N <- Big]
    ++ [try_(fun() -> <<N:32/float>> end) || N <- [1 bsl 127, 1 bsl 128, 1 bsl 200]].
