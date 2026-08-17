# Architect Review — Pass 1

**Status: WATCH · Verdict: COMMENT (approve with addressed comments)**

## Architectural assessment
The plan is correctly scoped: #19 stays research/decision (no driver code), #18 is two reversible dry-run gates. Option A (single `docs/` doc + AGENTS.md pointer) is the right call — it satisfies the "document approaches + privileges/deployment" depth requirement without bloating the agent-context file. The placement insight for the `agentdir-cli` dry-run (after the sparse-index wait) is architecturally essential and the plan captures it.

## Steelman of the rejected path (antithesis)
The strongest case for **Option B (expand AGENTS.md in place)**: the repo has no `docs/` tree, contributors already read AGENTS.md, and a single file means zero drift risk. If the feasibility content were short, B would win on simplicity.
**Why it still loses:** issue #19's acceptance demands per-platform privileges/deployment detail and an explicit per-mechanism normal-file-without-copy verdict. That is a page of structured content; embedding it in AGENTS.md degrades AGENTS.md's role as concise operating context. The plan's mitigation (AGENTS.md becomes a pointer, doc holds the body) resolves the drift concern B was protecting.

## Real tradeoff tension
**Decision recommendation vs. issue intent.** The plan recommends "keep as documented limitation now; optional future project for FUSE+ProjFS behind a feature flag." That is defensible and matches AGENTS.md's out-of-scope stance — but issue #19 asks to *"Decide whether this should become an implementation project or remain documented as a limitation."* The risk is the doc hedging instead of deciding. **Requirement:** the `## Decision` section must give ONE primary verdict in a single sentence (e.g. "Remain a documented limitation; do not start an implementation project now"), then list the trigger conditions that would flip it. A clear decision with conditions is fine; a non-committal "maybe later" is not.

## Comments to address (non-blocking)
1. **C1 (must address in doc):** Decision section must lead with one unambiguous verdict sentence, not a conditional. Acceptable to follow with "revisit if X".
2. **C2 (correctness):** In the per-mechanism verdict, explicitly note the bind-mount limitation — a plain read-only bind mount mirrors the source layout and **cannot produce agentdir's arbitrary restructured virtual tree** without an overlay/FUSE layer. This is the central reason bind mount is not a general answer; the doc must state it so the recommendation is honest.
3. **C3 (T5 robustness):** If T5 asserts ordering by string index, ensure it compares the index of the dry-run step vs the publish step within the SAME file, and guard against the substring also appearing in a comment. A simple "contains" assertion is acceptable and less brittle; ordering assertion is optional — prefer the simpler contains-check unless ordering regressions are a real concern.
4. **C4 (consistency):** macOS row should note Btrfs/XFS also provide reflink on Linux (CoW is not macOS-only), so the "non-CoW" framing is filesystem-level, not OS-level. Keep the platform table but add this nuance to avoid an inaccurate claim.

## No blocking architectural issues
Scope, file placement, verification approach, and the no-publish guardrail are sound. Proceed to Critic.
