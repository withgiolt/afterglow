-module(m3).
-export([t_big/0, t_funs/0, t_exc/0, t_bigmap/0]).

t_big() ->
    P = 1 bsl 200,
    {P, P * P - 1, P div 7, P rem 1000007, -P bsr 3, P band (P - 1), integer_to_binary(P),
     binary_to_integer(<<"-123456789012345678901234567890">>), P / 3, abs(-P), P > 1.0e60,
     trunc(1.0e30), round(-2.5), 7 div -2, -7 rem 2, math:pow(2, 10), float_to_binary(0.1, [short])}.

t_funs() ->
    Add = fun(A, B) -> A + B end,
    N = 10,
    Cl = fun(X) -> X * N end,
    Ext = fun lists:reverse/1,
    {Add(1, 2), Cl(4), Ext([1, 2, 3]), lists:map(Cl, [1, 2]), erlang:fun_info(Add, arity),
     apply(lists, seq, [1, 4]), apply(Cl, [5]), (fun F(0) -> 0; F(K) -> K + F(K - 1) end)(10),
     is_function(Cl, 1), lists:foldl(fun erlang:'+'/2, 0, [1, 2, 3])}.

t_exc() ->
    R1 = try throw(t) catch throw:V -> {thrown, V} end,
    R2 = try error({my, err}) catch error:E -> E end,
    R3 = try exit(bye) catch exit:X -> X end,
    R4 = try 1 / zero() catch error:badarith -> badarith end,
    R5 = try [] = [x] catch error:{badmatch, B} -> B end,
    R6 = try nested() catch C:Rs -> {C, Rs} end,
    R7 = try ok after put(k, done) end,
    R8 = try erlang:raise(error, re, []) catch error:re -> reraised end,
    R9 = try lists:nth(5, [a]) catch error:function_clause -> fc end,
    R10 = try undefined_mod:f() catch error:undef -> undef end,
    {R1, R2, R3, R4, R5, R6, R7, get(k), R8, R9, R10}.

zero() -> 0.
nested() ->
    try throw(inner) catch throw:inner -> erlang:error(outer) end.

t_bigmap() ->
    M = maps:from_list([{integer_to_binary(I), I} || I <- lists:seq(1, 100)]),
    {map_size(M), maps:get(<<"77">>, M), lists:sum(maps:values(M)), maps:is_key(<<"101">>, M)}.
