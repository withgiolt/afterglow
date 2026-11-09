%% Counter gen_server, version 1: the state is an integer.
-module(counter).
-behaviour(gen_server).
-export([init/1, handle_call/3, handle_cast/2, code_change/3]).

init([]) -> {ok, 0}.
handle_call(incr, _From, N) -> {reply, {v1, N + 1}, N + 1};
handle_call(get, _From, N) -> {reply, {v1, N}, N}.
handle_cast(_, N) -> {noreply, N}.
code_change(_Old, N, _Extra) -> {ok, N}.
