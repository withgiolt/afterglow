%% Integer text in every base: badarg like erl, never a trap.
-module(numx).
-export([t_bases/0]).

try_(F) -> try F() of V -> {ok, V} catch C:R -> {C, R} end.

t_bases() ->
    [try_(fun() -> Op(Arg, B) end)
     || B <- [-1, 0, 1, 2, 16, 36, 37, 1 bsl 40, x],
        {Op, Arg} <- [{fun binary_to_integer/2, <<"11">>}, {fun list_to_integer/2, "11"},
                      {fun integer_to_binary/2, 11}, {fun integer_to_list/2, 11}]]
    ++ [try_(fun() -> string:to_integer(S) end) || S <- ["12ab", "-7", "x", <<"42 ">>]].
