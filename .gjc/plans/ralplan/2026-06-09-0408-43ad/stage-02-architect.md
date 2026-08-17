# Architect Review — Pass 2 (rev 2)

**Status: CLEAR · Verdict: APPROVE**

All pass-1 comments are resolved in rev 2:
- **C1** — `## Decision` now opens with one unambiguous verdict sentence + trigger conditions. Resolved.
- **C2** — bind-mount "cannot restructure into arbitrary virtual tree without overlay/FUSE" is now an explicit required statement in the normal-file-compat section and T1 acceptance. Resolved.
- **C3** — T5 now specifies plain `in` contains-checks, no brittle index ordering. Resolved.
- **C4** — reflink CoW framed as filesystem-level (APFS/Btrfs/XFS), not macOS-only. Resolved.

Scope, placement (Option A), ordering correctness of the `agentdir-cli` dry-run (after sparse-index wait), and the no-publish guardrail are all sound. The ADR is complete (Decision/Drivers/Alternatives/Why/Consequences/Follow-ups) and correctly records why the "drop --metadata-only" alternative was rejected. No architectural concerns remain. Proceed to Critic.
