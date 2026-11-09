%% Counter gen_server, version 2: the state becomes #{count => N}; code_change converts.
-module(counter).
-behaviour(gen_server).
-export([init/1, handle_call/3, handle_cast/2, code_change/3]).

init([]) -> {ok, #{count => 0}}.
handle_call(incr, _From, #{count := N} = S) -> {reply, {v2, N + 1}, S#{count := N + 1}};
handle_call(get, _From, #{count := N} = S) -> {reply, {v2, N}, S}.
handle_cast(_, S) -> {noreply, S}.
code_change(_Old, N, _Extra) when is_integer(N) -> {ok, #{count => N}};
code_change(_Old, S, _Extra) -> {ok, S}.
