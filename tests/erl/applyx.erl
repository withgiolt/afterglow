%% apply/2,3 with argument counts around the X register limit (1024).
-module(applyx).
-export([t_limit/0]).

%% Only the reason's tag: afterglow does not report badarity's arguments.
class(F) -> try F() catch C:R when is_tuple(R) -> {C, element(1, R)}; C:R -> {C, R} end.

t_limit() ->
    [{N, class(fun() -> apply(erlang, max, lists:seq(1, N)) end),
         class(fun() -> apply(fun lists:max/1, lists:seq(1, N)) end)}
     || N <- [1022, 1023, 1024, 2000]].
