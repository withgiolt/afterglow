-module(m2b).
-export([gc1/0, gc2/0, gc3/0, cp/0, sl/0]).
gc1() -> unicode_util:gc(<<" hello">>).
gc2() -> unicode_util:gc(<<"hello">>).
gc3() -> unicode_util:gc([<<"ab">>]).
cp() -> unicode_util:cp(<<"é!">>).
sl() -> string:slice(<<"héllo"/utf8>>, 2, 3).
