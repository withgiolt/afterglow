%% P4: timers that outlive the Durable Object instance, and Cron ticks.
-module(tm).
-export([arm/1, result/0, ticker/0, ticks/0]).
-export([sleeper/1, tick_loop/1]).

arm(Ms) ->
    register(sleeper, spawn(?MODULE, sleeper, [Ms])),
    armed.

sleeper(Ms) ->
    T0 = os:system_time(millisecond),
    erlang:send_after(Ms, self(), wake),
    wait(T0).

wait(T0) ->
    receive
        wake -> keep({fired, os:system_time(millisecond) - T0});
        {result, From} -> From ! waiting, wait(T0)
    end.

keep(R) -> receive {result, From} -> From ! R, keep(R) end.

result() ->
    whereis(sleeper) ! {result, self()},
    receive R -> R after 1 -> busy end.

ticker() ->
    register(ticker, spawn(?MODULE, tick_loop, [[]])),
    ticking.

tick_loop(Ticks) ->
    receive
        {tick, Cron} -> tick_loop([Cron | Ticks]);
        {result, From} -> From ! {ticks, length(Ticks), lists:reverse(Ticks)}, tick_loop(Ticks)
    end.

ticks() ->
    whereis(ticker) ! {result, self()},
    receive R -> R after 1 -> busy end.
