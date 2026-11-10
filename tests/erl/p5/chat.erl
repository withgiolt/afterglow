%% P5: a chat room process owning WebSockets. Its state (members, message count) lives
%% in the process, so it survives hibernation only through the checkpoint.
-module(chat).
-export([start/0]).

start() -> loop([], 0).

loop(Members, N) ->
    receive
        {websocket, Ref, open} ->
            afterglow_runtime:ws_send(Ref, [<<"welcome ">>, integer_to_binary(id(Ref))]),
            loop([Ref | Members], N);
        {websocket, Ref, {text, Text}} ->
            Line = iolist_to_binary([integer_to_binary(id(Ref)), <<": ">>, Text,
                                     <<" (members ">>, integer_to_binary(length(Members)),
                                     <<", messages ">>, integer_to_binary(N + 1), <<")">>]),
            [afterglow_runtime:ws_send(M, Line) || M <- Members],
            loop(Members, N + 1);
        {websocket, Ref, closed} ->
            loop(lists:delete(Ref, Members), N);
        {websocket, _, _} ->
            loop(Members, N)
    end.

id({afterglow_ws, Id}) -> Id.
