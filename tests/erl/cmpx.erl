%% Exact comparison and map key order: integers before floats, -0.0 apart from 0.0,
%% external funs that differ only in arity.
-module(cmpx).
-export([t_keys/0, t_zero/0, t_funs/0]).

t_keys() ->
    M = #{2 => a, 1.5 => b, 1 => c, 1.0 => d, -1 => e, 3.5 => f},
    [maps:keys(M), maps:to_list(M), maps:fold(fun(K, _, A) -> [K | A] end, [], M),
     #{1 => a} < #{1.0 => a}, #{2 => a} < #{1.0 => a}, 1 =:= 1.0, 1 == 1.0, 2 < 1.5,
     binary_to_term(term_to_binary(M)) =:= M].

t_zero() ->
    <<Z/float>> = <<128, 0, 0, 0, 0, 0, 0, 0>>,
    [Z =:= +0.0, Z =/= +0.0, Z == +0.0, Z < +0.0, maps:size(#{Z => a, +0.0 => b}),
     maps:keys(#{+0.0 => a, Z => b}), {Z} =:= {+0.0}, [Z] == [+0.0]].

t_funs() ->
    [F2, F3] = [erlang:make_fun(lists, map, A) || A <- [2, 3]],
    [F2 =:= F3, F2 == F3, F2 < F3, maps:size(#{F2 => a, F3 => b}), lists:sort([F3, F2]) =:= [F2, F3]].
