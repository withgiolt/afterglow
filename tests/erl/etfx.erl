%% binary_to_term on malformed or unusual input: badarg like erl, never a trap.
-module(etfx).
-export([t_decode/0]).

decode(B) ->
    try binary_to_term(B) of T -> {ok, T} catch C:R -> {C, R} end.

t_decode() ->
    [is_reference(element(2, decode(<<131, 90, 0, 0, 119, 0, 0, 0, 0, 0>>))),
     decode(<<131, 77, 0, 0, 0, 1, 200, 255>>), decode(<<131, 77, 0, 0, 0, 1, 0, 255>>),
     decode(<<131, 77, 0, 0, 0, 0, 3>>), decode(<<131, 77, 0, 0, 0, 1, 3, 255>>),
     decode(<<131, 108, 255, 255, 255, 255, 106>>), decode(<<131, 116, 255, 255, 255, 255>>),
     decode(<<131, 105, 255, 255, 255, 255>>), decode(<<131, 109, 255, 255, 255, 255>>)].
