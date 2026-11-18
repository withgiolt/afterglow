-module(envx).
-export([t_put/0, t_unset/0, t_badarg/0]).

t_put() ->
    Name = "AFTERGLOW_ENV_TEST",
    true = os:putenv(Name, "first"),
    First = os:getenv(Name),
    true = os:putenv(Name, "second=値"),
    Second = os:getenv(Name),
    Entries = [E || E <- os:getenv(), E =:= Name ++ "=second=値"],
    true = os:putenv(Name, ""),
    Empty = os:getenv(Name),
    true = os:unsetenv(Name),
    true = os:putenv("", "empty name"),
    EmptyName = os:getenv(""),
    true = os:unsetenv(""),
    {First, Second, Entries, Empty, EmptyName}.

t_unset() ->
    Name = "AFTERGLOW_ENV_UNSET",
    Other = "AFTERGLOW_ENV_KEEP",
    true = os:putenv(Name, "remove"),
    true = os:putenv(Other, "keep"),
    true = os:unsetenv(Name),
    Missing = os:getenv(Name),
    true = os:unsetenv(Name),
    Kept = os:getenv(Other),
    true = os:unsetenv(Other),
    {Missing, Kept}.

call(Op, Args) ->
    try apply(os, Op, Args) catch C:R -> {C, R} end.

t_badarg() ->
    Name = "AFTERGLOW_ENV_BADARG",
    true = os:putenv(Name, "unchanged"),
    Bad = [atom, 42, [atom], [97 | 98], [-1], [16#110000], [16#D800], [0], "a" ++ [0] ++ "b"],
    Names = ["a=b" | Bad],
    Results = [{call(putenv, [N, "value"]), call(unsetenv, [N])} || N <- Names],
    Values = [call(putenv, [Name, V]) || V <- Bad],
    Kept = os:getenv(Name),
    true = os:unsetenv(Name),
    {Results, Values, Kept}.
