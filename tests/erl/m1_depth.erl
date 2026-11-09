-module(m1_depth).
-export([d10k/0, d100k/0, d1m/0, map100k/0]).
len([]) -> 0;
len([_ | T]) -> 1 + len(T).
d10k() -> len(lists:seq(1, 10000)).
d100k() -> len(lists:seq(1, 100000)).
d1m() -> len(lists:seq(1, 1000000)).
map100k() -> lists:sum(lists:map(fun(X) -> X * 2 end, lists:seq(1, 100000))).
