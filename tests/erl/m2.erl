-module(m2).
-compile(nowarn_deprecated_catch).
-export([t_match/0, t_split/0, t_bins/0, t_utf/0, t_build/0, t_ints/0, t_unicode/0, t_maps/0]).

t_match() ->
    P = binary:compile_pattern(<<"=">>),
    {binary:match(<<"a=b">>, P), binary:match(<<"a=b">>, <<"=">>), binary:match(<<"abc">>, [<<"b">>, <<"bc">>]),
     binary:matches(<<"a,b,,c">>, <<",">>), binary:match(<<"abcabc">>, <<"c">>, [{scope, {3, 3}}])}.

t_split() ->
    {binary:split(<<"a=b=c">>, <<"=">>), binary:split(<<"a=b=c">>, <<"=">>, [global]),
     binary:split(<<",a,,b,,">>, <<",">>, [global, trim_all]), string:split(<<"key=value">>, "="),
     string:split("a b c", " ", all), string:lexemes(<<"  hello   world ">>, " ")}.

t_bins() ->
    B = <<1, 2, 3, 4, 5, 6, 7, 8, 9>>,
    <<A:8, Rest/binary>> = B,
    <<X:16/little, Y:3, Z:5, _/bitstring>> = Rest,
    {A, Rest, X, Y, Z, byte_size(B), bit_size(<<1:3>>), binary_part(B, 2, 3), binary:part(B, {1, 2}),
     binary_to_list(B), list_to_binary([1, [2, <<3>>], 4]), iolist_to_binary(["ab", $c, <<"d">>]),
     <<(-1):12/signed, 7:4>>, <<255:16/little>>, << <<C>> || <<C>> <= <<"xyz">> >>}.

t_utf() ->
    S = <<"héllo wörld ✓"/utf8>>,
    Cps = [C || <<C/utf8>> <= S],
    {Cps, <<<<C/utf8>> || C <- Cps>>, <<16#1F600/utf8>>, <<16#1F600/utf16>>, <<16#1F600/utf32-little>>,
     string:length(S), string:uppercase(S), string:reverse(S), string:slice(S, 2, 3)}.

t_build() ->
    L = lists:seq(1, 300),
    Bin = lists:foldl(fun(I, Acc) -> <<Acc/binary, I:16>> end, <<>>, L),
    Bin2 = lists:foldl(fun(I, Acc) -> <<Acc/binary, (integer_to_binary(I))/binary, ",">> end, <<>>, L),
    <<X:64/float>> = <<2.25/float>>,
    {byte_size(Bin), binary:last(Bin), binary:part(Bin2, 0, 20), byte_size(Bin2),
     <<1.5/float>>, <<1.5:32/float>>, X}.

t_ints() ->
    Big = 1 bsl 100 + 12345,
    <<Big2:104>> = <<Big:104>>,
    <<S:72/signed-little>> = <<(-Big div 1000000000000000000):72/signed-little>>,
    {Big2, S, <<Big:104/little>>, binary:encode_unsigned(Big), binary:decode_unsigned(<<1, 0, 0>>)}.

t_unicode() ->
    {unicode:characters_to_binary("héllo"), unicode:characters_to_binary([<<"a">>, "b", [$c]]),
     unicode:characters_to_list(<<"wörld"/utf8>>), unicode:characters_to_binary(<<"ab", 255>>),
     unicode:characters_to_binary(<<"ab", 195>>), unicode:characters_to_binary("abc", latin1),
     try unicode:characters_to_binary(foo) catch C1:R1 -> {C1, R1} end}.

t_maps() ->
    M = #{a => 1, b => 2},
    M2 = M#{c => 3, a => 10},
    #{c := C} = M2,
    M3 = maps:from_list([{I, I * I} || I <- lists:seq(1, 20)]),
    {M2, C, maps:get(b, M), maps:find(x, M), lists:sort(maps:keys(M2)), maps:to_list(M3), maps:fold(fun(K, V, A) -> K + V + A end, 0, M3),
     maps:map(fun(_, V) -> V + 1 end, M), maps:filter(fun(K, _) -> K > 18 end, M3), map_size(M3),
     try M#{zz := 1} catch C2:R2 -> {C2, R2} end, maps:merge(M, #{d => 4}), maps:remove(a, M), is_map_key(a, M)}.
