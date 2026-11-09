#!/usr/bin/env escript
%% Survey: reachable functions from Mod:start/0 across the fixture build and OTP,
%% the opcodes they use, and the external calls with no .beam to compile (BIFs
%% and NIFs the runtime must provide). Uses beam_disasm; a design aid only.
%% Usage: tools/reach.escript <ebin-root> <Module> [Function Arity]
-mode(compile).

main([Root, ModS | Rest]) ->
    {F, A} = case Rest of [Fs, As] -> {list_to_atom(Fs), list_to_integer(As)}; [] -> {start, 0} end,
    Shims = [base64, io_lib_format, json, maps, rand, string, uri_string],
    Paths = [P || P <- filelib:wildcard(Root ++ "/*/ebin")],
    put(paths, Paths), put(shims, Shims),
    walk([{list_to_atom(ModS), F, A}], #{}),
    report().

find_beam(M) ->
    Local = [P ++ "/" ++ atom_to_list(M) ++ ".beam" || P <- get(paths),
             not (lists:member(M, get(shims)) andalso lists:suffix("glowvm/ebin", P))],
    case [B || B <- Local, filelib:is_file(B)] of
        [B | _] -> B;
        [] -> case code:which(M) of
                  W when is_list(W) -> W;
                  preloaded -> code:lib_dir(erts) ++ "/ebin/" ++ atom_to_list(M) ++ ".beam";
                  _ -> none
              end
    end.

module(M) ->
    case get({mod, M}) of
        undefined ->
            R = case find_beam(M) of
                    none -> none;
                    B -> case beam_disasm:file(B) of
                             {beam_file, M, Exps, _Attr, _CI, Code} -> {Exps, Code, B};
                             _ -> none
                         end
                end,
            put({mod, M}, R), R;
        R -> R
    end.

walk([], Seen) -> put(seen, Seen);
walk([MFA | Q], Seen) when is_map_key(MFA, Seen) -> walk(Q, Seen);
walk([{M, F, A} = MFA | Q], Seen) ->
    case erlang:is_builtin(M, F, A) orelse module(M) of
        true -> bump(missing, MFA), walk(Q, Seen#{MFA => missing});
        none -> bump(missing, MFA), walk(Q, Seen#{MFA => missing});
        {_Exps, Code, _} ->
            case [Is || {function, F1, A1, _, Is} <- Code, F1 =:= F, A1 =:= A] of
                [] -> bump(missing, MFA), walk(Q, Seen#{MFA => missing});
                [Is] ->
                    Next = lists:flatmap(fun(I) -> targets(M, Code, I) end, Is),
                    [bump(op, element(1, I)) || I <- Is, is_tuple(I)],
                    [bump(op, I) || I <- Is, is_atom(I)],
                    bump(ninstr, M, length(Is)),
                    walk(Next ++ Q, Seen#{MFA => ok})
            end
    end.

targets(M, _Code, I) ->
    case I of
        {call, _, {M2, F, A}} -> [{M2, F, A}];
        {call_only, _, {M2, F, A}} -> [{M2, F, A}];
        {call_last, _, {M2, F, A}, _} -> [{M2, F, A}];
        {call_ext, _, {extfunc, M2, F, A}} -> [{M2, F, A}];
        {call_ext_only, _, {extfunc, M2, F, A}} -> [{M2, F, A}];
        {call_ext_last, _, {extfunc, M2, F, A}, _} -> [{M2, F, A}];
        {make_fun3, {M2, F, A}, _, _, _, _} -> [{M2, F, A}];
        {make_fun2, {M2, F, A}, _, _, _} -> [{M2, F, A}];
        {bif, Name, _, Args, _} -> bump(bif, {Name, length(Args)}), [];
        {gc_bif, Name, _, _, Args, _} -> bump(bif, {Name, length(Args)}), [];
        {apply, _} -> bump(apply, M), [];
        {apply_last, _, _} -> bump(apply, M), [];
        _ -> lits(I)
    end.

%% external fun literals: fun m:f/a appear as {literal, Fun}
lits(T) when is_function(T) ->
    case erlang:fun_info(T, type) of
        {type, external} ->
            {module, M} = erlang:fun_info(T, module), {name, F} = erlang:fun_info(T, name),
            {arity, A} = erlang:fun_info(T, arity), [{M, F, A}];
        _ -> []
    end;
lits(T) when is_tuple(T) -> lists:flatmap(fun lits/1, tuple_to_list(T));
lits(T) when is_list(T) -> lists:flatmap(fun lits/1, T);
lits(T) when is_map(T) -> lits(maps:to_list(T));
lits(_) -> [].

bump(K, V) -> bump(K, V, 1).
bump(K, V, N) -> put({K, V}, case get({K, V}) of undefined -> N; X -> X + N end).

report() ->
    Seen = get(seen),
    Ok = [MFA || {MFA, ok} <- maps:to_list(Seen)],
    Missing = lists:sort([MFA || {MFA, missing} <- maps:to_list(Seen)]),
    Mods = lists:usort([M || {M, _, _} <- Ok]),
    Instr = lists:sum([N || {{ninstr, _}, N} <- get()]),
    io:format("reachable functions: ~p in ~p modules, ~p instructions~n", [length(Ok), length(Mods), Instr]),
    io:format("modules: ~p~n", [Mods]),
    Ops = lists:reverse(lists:keysort(2, [{Op, N} || {{op, Op}, N} <- get()])),
    io:format("opcodes (~p): ~p~n", [length(Ops), Ops]),
    io:format("guard/gc bifs: ~p~n", [lists:sort([B || {{bif, B}, _} <- get()])]),
    io:format("no beam (runtime must provide) (~p): ~p~n", [length(Missing), Missing]),
    io:format("apply in: ~p~n", [[M || {{apply, M}, _} <- get()]]),
    Big = lists:sublist(lists:reverse(lists:keysort(2, [{M, N} || {{ninstr, M}, N} <- get()])), 12),
    io:format("biggest modules by reached instructions: ~p~n", [Big]).
