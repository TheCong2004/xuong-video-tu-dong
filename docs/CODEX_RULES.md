# Floword migration rules

- Floword Studio remains one workspace: Project Brief, Pipeline Progress, Run Console, Configure.
- CapCut Automation and Image Editor remain separate applications. Image Editor is out of scope.
- Preserve business capabilities through `REUSE -> WRAP -> ADAPT`; do not copy the six legacy app navigations.
- Canonical owners: OmniRoute (AI), MediaCrawler (research), Youwee (download), Vynaro (analysis), Story Studio (planning), OpenMontage (timeline), CapCut backend (draft/render).
- Rust pipeline worker owns job state. The frontend owns input/display state only.
- Never expose raw credentials, cookies, tokens, passwords, or browser profiles in UI logs or LocalStorage.
- Never claim a runtime capability retained without a real operation and validated output.
- Do not delete legacy source or hide launcher entries until the retention gate passes.
- Preserve historical job deserialization with additive/defaulted DTO fields.
- No destructive git operations and no automatic commit.

