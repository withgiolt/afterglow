Same app on both runtimes, responses byte-identical. Median of 3 runs. MINGW64_NT-10.0-26200 x86_64, AMD Ryzen 5 5600G with Radeon Graphics, OTP 29 (jit), Node v24.18.0, afterglow 9e93132.
Bare boot, no app: erl 378 ms, node 221 ms.

| Fixture | Warm per request: afterglow / erl | Cold, process start to response: afterglow / erl | afterglow init | Memory: afterglow / erl |
| --- | --- | --- | --- | --- |
| basic_get (n=20000) | 9.9 µs / 3.2 µs | 280 ms / 633 ms | 5.9 ms | 0.5 MiB / 32.4 MiB |
| lustre_html (n=20000) | 12.3 µs / 5.0 µs | 254 ms / 669 ms | 4.4 ms | 0.5 MiB / 32.6 MiB |
| json_roundtrip (n=1000) | 4.72 ms / 3.50 ms | 307 ms / 616 ms | 4.9 ms | 4.9 MiB / 33.7 MiB |
