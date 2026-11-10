%% unicode:characters_to_binary/list on bad input: the Rest they return must have
%% erl's shape, so a caller can resume from it. Not yet like erl, so not here: a
%% truncated sequence before more input, and empty tails around the bad element.
-module(unix).
-export([t_rest/0]).

t_rest() ->
    Inputs = [[[[<<255>>, $x], $y], $z], [<<"ok">>, <<255>>, <<"tail">>], [[$a, -1], $b],
              [[], [<<255>>, []], []], <<"ab", 255, "cd">>, [<<"ab">> | <<255>>]],
    [{unicode:characters_to_binary(I), unicode:characters_to_list(I),
      unicode:characters_to_binary(I, latin1)} || I <- Inputs].
