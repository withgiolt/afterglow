# Goal for the full spike run (set by the user on 2026-11-09)

/goal Run the GlowVM AOT spike in ~/Developer/giolt/glowc-phase0, following SPIKE.md,
until the M5 benchmark gate has a measured verdict.

Done means:
- M1–M4 built in order, each with its difftest passing against real `erl` (OTP 29)
- M4: glowvm's smoke fixtures produce byte-identical responses under glowc and glowvm on workerd
- M5: glowc and glowvm deployed side by side on the Giolt account,
  basic_get, lustre_html and json_roundtrip measured with the Phase 0 method
  (Analytics CPU, 1k warm-up + 5k measured, 3 runs)
- Verdict against the first table in SPIKE.md: pass, or which row failed and by how much
- SPIKE.md, PROGRESS.md and report.html updated with the results

If M5 passes, continue into Phase 2 (P1–P5) the same way.
Stop early, and report, if a kill criterion in SPIKE.md triggers.

Rules: glowvm stays untouched. No remote, no push. Commits without attribution lines.
Stay under 40k Worker requests per day on Giolt, which shares the Free plan's limit with production.
Delete every Worker after measuring it. Keep PROGRESS.md current after each step.
