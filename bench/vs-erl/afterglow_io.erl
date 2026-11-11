-module(afterglow_io).

%% Loaded over the real afterglow_io for the erl side of bench/vs-erl.sh. Under
%% afterglow the runtime replaces these two functions with the request buffer
%% and the response buffer, so the erl side reads the request from memory and
%% keeps the response there too. Otherwise erl would also pay for reading stdin
%% and writing stdout on every request.

-export([read_stdin/0, write_stdout/1]).

read_stdin() -> persistent_term:get(bench_request).

write_stdout(Data) ->
    put(bench_response, Data),
    nil.
