%% Tier 2 turn entry. A turn's input (ETF) is one of:
%%   {call, Name, Mod, Msg}: gen_server:call to Name (started from Mod if missing)
%%   {apply, M, F, A}: apply(M, F, A) in the turn's process
%%   {deliver, [{Dest, Msg}]}: messages from other nodes (DOs)
%%   {noconnection, Node}: the host could not deliver to Node
%% and the turn's output is the result as ETF. Before the
%% turn ends, every changed gen-style process has its state snapshotted, so it can be
%% restarted through code_change after a redeploy (upgrade/3).
-module(glowc_node).
-export([turn/0, upgrade/3]).

turn() ->
    Reply = case binary_to_term(host_io:read_stdin()) of
        {call, Name, Mod, Msg} ->
            case whereis(Name) of
                undefined -> {ok, _} = gen_server:start({local, Name}, Mod, [], []);
                _ -> ok
            end,
            gen_server:call(Name, Msg);
        {apply, M, F, A} ->
            apply(M, F, A);
        %% messages from other nodes; the receivers run after this process ends
        {deliver, Msgs} ->
            [glowc:node_deliver(D, M) || {D, M} <- Msgs],
            ok;
        {noconnection, Node} ->
            glowc:node_noconnection(Node)
    end,
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
