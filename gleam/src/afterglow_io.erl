-module(afterglow_io).

%% The request and response of one bridge call. Under afterglow the runtime
%% replaces both functions (afterglow_io:read_stdin/0 and write_stdout/1 in
%% crates/afterglow_runtime/src/bifs.rs) with the request binary and the
%% response buffer. Under real erl, which the difftests compare against, these
%% bodies run and use stdin and stdout.

-export([read_stdin/0, write_stdout/1]).

-define(MAX_INPUT_SIZE, 10 * 1024 * 1024).

read_stdin() ->
    %% The payload is ETF, so read raw bytes: line-based, unicode-decoded
    %% reads would mangle anything >= 0x80.
    ok = io:setopts(standard_io, [binary, {encoding, latin1}]),
    read_loop(<<>>).

read_loop(Acc) ->
    case file:read(standard_io, 65536) of
        eof -> Acc;
        {error, _} -> Acc;
        {ok, Bin} ->
            NewAcc = <<Acc/binary, Bin/binary>>,
            case byte_size(NewAcc) > ?MAX_INPUT_SIZE of
                true -> erlang:error(input_too_large);
                false -> read_loop(NewAcc)
            end
    end.

write_stdout(Data) when is_binary(Data) ->
    ok = io:setopts(standard_io, [binary, {encoding, latin1}]),
    try file:write(standard_io, Data) of
        ok -> ok;
        _ -> error
    catch _:_ -> error
    end;
write_stdout(_Data) ->
    erlang:error(badarg).
