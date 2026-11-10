%% unicode:characters_to_binary/list on bad input: the Rest they return must have
%% erl's shape, so a caller can resume from it. A UTF-8 sequence may be split across
%% binaries. Not yet like erl, so not here: empty tails around the bad element.
-module(unix).
-export([t_rest/0, t_split/0]).

t_rest() ->
    Inputs = [[[[<<255>>, $x], $y], $z], [<<"ok">>, <<255>>, <<"tail">>], [[$a, -1], $b],
              [[], [<<255>>, []], []], <<"ab", 255, "cd">>, [<<"ab">> | <<255>>]],
    [{unicode:characters_to_binary(I), unicode:characters_to_list(I),
      unicode:characters_to_binary(I, latin1)} || I <- Inputs].

t_split() ->
    Inputs = [[<<226>>, <<130, 172>>], [<<"a", 226>>, <<130>>, <<172, "b">>], [<<226, 130>>, 172],
              [$a, [<<"b">>, <<226, 130>>], $c], [<<226>>], [<<"x", 226, 130>>], [<<226>>, <<"x">>],
              [<<226>>, <<130>>], [<<226>>, <<130>>, $z], [<<226>>, [], <<130, 172>>],
              [[<<226>>], [<<130, 172>>]], [<<226>>, <<255>>], [<<240, 159>>, <<152>>, <<128>>],
              <<"ok", 226>>, [<<"ab">> | <<226, 130>>]],
    [{unicode:characters_to_binary(I), unicode:characters_to_list(I)} || I <- Inputs].
