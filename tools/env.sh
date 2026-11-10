# Source this on Windows (Git Bash) before running the tools: puts Erlang/OTP and
# Binaryen on PATH. A no-op where they are already on PATH (macOS, Linux).
# Erlang: the newest erts-*/bin, not OTP's bin/, whose launchers need an erl.ini that a
# silent winget install doesn't write (without it erl.exe crashes on start).
otp="/c/Program Files/Erlang OTP"
if [[ -d "$otp" ]]; then
  erts=$(ls -d "$otp"/erts-* 2>/dev/null | sort -V | tail -1)
  [[ -n "$erts" ]] && PATH="$erts/bin:$PATH"
fi
for d in "$HOME"/.local/opt/binaryen-*/bin; do
  [[ -d "$d" ]] && PATH="$d:$PATH"
done
export PATH
