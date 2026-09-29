# Agent Workflow Rules

- **The Roadmap Is The Plan**: read [`ROADMAP.md`](./ROADMAP.md) first, every
  session — especially "What is really shipped" and the changelog tail. All
  work follows it: nothing is built, removed or changed outside its items.
  When a decision is made in conversation, record it in the roadmap (item +
  changelog row) even if the owner does not ask, then say that it was
  recorded. Never quote phase numbers or status from memory — the old P0–P12
  plan in `docs/archive/` is history, not the plan.

- **Commit Before Testing**: ALWAYS commit and push your changes to the git repository after finishing a major implementation step, and specifically **before** asking the user to test or review the code. Do not leave uncommitted changes in the workspace when asking the user to run the application.

- **Continuous Documentation (Workspace Docs Folder)**: ALWAYS write and update plans, architectural discussions, roadmaps, and pitfalls as persistent markdown files directly in the repository's `docs/` directory (e.g., `docs/conversation.md`, `docs/pitfalls.md`, etc.). 
  - Do NOT use temporary `.gemini/antigravity/brain/` artifacts for permanent project documentation. 
  - Document ongoing steps, dead-ends, and failures directly into the respective files in `docs/` before pivoting to new strategies.

- **Strict Developer Invariants**: ALWAYS follow strict developer invariants specified in [`docs/WORKING.md`](./docs/WORKING.md) (zero `unwrap()` in production, sidecar backup sync on DB updates, no root scratch files).

