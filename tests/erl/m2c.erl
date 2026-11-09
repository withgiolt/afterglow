-module(m2c).
-export([lex/0, lex2/0, sp/0, ul/0]).
lex() -> string:lexemes(<<"  hello   world ">>, " ").
lex2() -> {string:split("a b c", " ", all), string:lexemes(<<"  hello   world ">>, " ")}.
sp() -> string:split(<<"key=value">>, "=").
ul() -> {string:length(<<"héllo"/utf8>>), string:slice(<<"héllo"/utf8>>, 2, 3)}.
