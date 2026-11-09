-module(m1).
-export([fib_test/0, sum_test/0, fib/1, mixed_test/0]).

fib(0) -> 0;
fib(1) -> 1;
fib(N) -> fib(N - 1) + fib(N - 2).

fib_test() -> fib(30).

sum_test() -> sum(lists:seq(1, 1000000), 0).

sum([], Acc) -> Acc;
sum([H | T], Acc) -> sum(T, Acc + H).

%% Exercises tuples, case, try/catch, big integers and lists functions.
mixed_test() ->
    L = [{N, N * N} || N <- lists:seq(1, 20)],
    Big = lists:foldl(fun({_, Sq}, A) -> A * Sq + 1 end, 1, L),
    Caught = try element(5, {a, b}) catch error:R -> {caught, R} end,
    Thrown = (catch throw(x)),
    Rev = lists:reverse([atom, 1.5, "str", <<"bin">>]),
    {length(L), Big, Caught, Thrown, Rev, lists:keyfind(7, 1, L)}.
