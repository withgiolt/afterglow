-module(m2d).
-export([a/0, b/0, c/0, d/0]).
a() -> string:slice(<<"héllo wörld ✓"/utf8>>, 2, 3).
b() -> string:slice(<<"héllo wörld"/utf8>>, 2, 3).
c() -> string:slice(<<"hello world abc"/utf8>>, 2, 3).
d() -> <<W:56, _/binary>> = <<"llo wor ld">>, {W, W band 16#80808080808080, (W + 16#01010101010101) band 16#80808080808080}.
