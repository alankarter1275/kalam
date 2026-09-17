# Agent Workflow Rules

- **Commit Before Testing**: ALWAYS commit and push your changes to the git repository after finishing a major implementation step, and specifically **before** asking the user to test or review the code. Do not leave uncommitted changes in the workspace when asking the user to run the application.

- **Continuous Documentation (Workspace Docs Folder)**: ALWAYS write and update plans, architectural discussions, roadmaps, and pitfalls as persistent markdown files directly in the repository's `docs/` directory (e.g., `docs/p7-plan.md`, `docs/pitfalls.md`, etc.). 
  - Do NOT use temporary `.gemini/antigravity/brain/` artifacts for permanent project documentation. 
  - Document ongoing steps, dead-ends, and failures directly into the respective files in `docs/` before pivoting to new strategies.

- **Strict Developer Invariants**: ALWAYS follow strict developer invariants specified in [`docs/WORKING.md`](./docs/WORKING.md) (zero `unwrap()` in production, sidecar backup sync on DB updates, no root scratch files).

