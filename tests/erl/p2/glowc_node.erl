%% Tier 2 turn entry. Each turn's input is term_to_binary({call, Name, Mod, Msg}):
%% the gen_server registered as Name (started from Mod if missing) gets Msg through
%% gen_server:call, and the reply is the turn's output (term_to_binary). Before the
%% turn ends, every changed gen-style process has its state snapshotted, so it can be
%% restarted through code_change after a redeploy (upgrade/3).
-module(glowc_node).
-export([turn/0, upgrade/3]).

turn() ->
    {call, Name, Mod, Msg} = binary_to_term(host_io:read_stdin()),
    case whereis(Name) of
        undefined -> {ok, _} = gen_server:start({local, Name}, Mod, [], []);
        _ -> ok
    end,
    Reply = gen_server:call(Name, Msg),
    snapshot(),
    host_io:write_stdout(term_to_binary(Reply)).

snapshot() ->
    [glowc:node_set_state(Pid, Mod, sys:get_state(Pid)) || {Pid, Mod} <- glowc:node_gen_servers()],
    ok.

%% Runs in a fresh process after a redeploy: the old state through Mod:code_change,
%% then the gen_server loop again under the same name.
upgrade(Mod, Name, State) ->
    {ok, State2} = Mod:code_change(old, State, []),
    put('$ancestors', [self()]),
    put('$initial_call', {Mod, init, 1}),
    case Name of
        undefined -> gen_server:enter_loop(Mod, [], State2);
        _ ->
            Self = self(),
            case whereis(Name) of
                Self -> ok;
                _ -> true = register(Name, Self)
            end,
            gen_server:enter_loop(Mod, [], State2, {local, Name})
    end.
