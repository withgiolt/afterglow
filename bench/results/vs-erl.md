Same app on both runtimes, responses byte-identical. Median of 5 runs. Linux x86_64, Intel(R) Xeon(R) Processor @ 2.10GHz, OTP 28 (jit), Node v24.22.1, afterglow 23f3187.
Bare boot, no app: erl 175 ms, node 30 ms.

| Fixture | Warm per request: afterglow / erl | Cold, process start to response: afterglow / erl | afterglow init | Memory: afterglow / erl |
| --- | --- | --- | --- | --- |
| basic_get (n=20000) | 10.7 µs / 3.4 µs | 60 ms / 219 ms | 8.6 ms | 0.5 MiB / 29.9 MiB |
| lustre_html (n=20000) | 12.7 µs / 6.5 µs | 57 ms / 271 ms | 10.0 ms | 0.5 MiB / 30.0 MiB |
| json_roundtrip (n=1000) | 5.52 ms / 2.59 ms | 108 ms / 248 ms | 7.0 ms | 4.9 MiB / 31.1 MiB |
