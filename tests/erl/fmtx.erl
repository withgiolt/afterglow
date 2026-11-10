%% io_lib:format directives one by one, and float_to_list(F, [short]).
-module(fmtx).
-export([t_directives/0, t_short/0]).

one(F, A) ->
    try iolist_to_binary(io_lib:format(F, A)) catch C:R -> {C, R} end.

t_directives() ->
    [one("~p", [{a, [1, 2], <<"x">>}]), one("~s", ["str"]), one("~.2f", [3.14159]),
     one("~w", ['quoted atom']), one("~8.3.0f", [2.5]), one("~-6s", ["ab"]), one("~b", [42]),
     one("~.16B", [255]), one("~e", [1.0e10]), one("~ts", ["ünï"]), one("~ts", [<<"ünï"/utf8>>]),
     one("~p", ["ünï"]), one("~tp", ["ünï"]), one("~p", [<<"ünï"/utf8>>]), one("~tp", [<<"ünï"/utf8>>]),
     one("~g", [0.5]), one("~10.3e", [123.456]), one("~c", [$a]), one("~5c", [$z]), one("~i~s", [x, "y"]),
     one("~x", [42, "0x"]), one("~.8#", [8]), one("~+", [1]), one("~P", [[1, 2, 3, 4, 5], 3]),
     one("~W", [{a, {b, {c}}}, 2]), one("~n", []), one("~~", []), one("~*s", [5, "x"]),
     one("~p", [#{a => 1}]), one("~p", [lists:seq(1, 40)]), one("~p", [3.0]), one("~w", [2500.0])].

t_short() ->
    [float_to_list(F, [short]) || F <- [2500.0, 100.0, 1000.0, 10000.0, 123456.0, 1.0e15, 1.0e16,
        0.1, 0.001, 0.0001, 1.0e-5, 1.5e-7, 25.0, 250.0, 2.5e5, 3.0e7, 1.2345e21, 12.5, 1.0e22,
        120.0, 1200.0, 0.0012, 123400.0, 1.0, 0.0, -2500.0, -0.0, 5.0e-324, 1.7976931348623157e308,
        0.30000000000000004, 1/3]].
