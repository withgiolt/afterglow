%% External funs inside a literal: the compiler must know their atoms.
-module(litfun).
-export([t/0]).

t() -> [F(11, 2) || F <- [fun integer_to_binary/2, fun integer_to_list/2]].
