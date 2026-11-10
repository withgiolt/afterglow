%% Exact comparison and map key order: integers before floats.
-module(cmpx).
-export([t_keys/0]).

t_keys() ->
    M = #{2 => a, 1.5 => b, 1 => c, 1.0 => d, -1 => e, 3.5 => f},
    [maps:keys(M), maps:to_list(M), maps:fold(fun(K, _, A) -> [K | A] end, [], M),
     #{1 => a} < #{1.0 => a}, #{2 => a} < #{1.0 => a}, 1 =:= 1.0, 1 == 1.0, 2 < 1.5,
     binary_to_term(term_to_binary(M)) =:= M].
