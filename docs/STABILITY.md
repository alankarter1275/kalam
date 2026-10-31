# API stability

This file assigns stability tiers to the local Chapbook crates. In this
workspace, Kalam is the main downstream app; the tiers show which engine
interfaces require extra care when they change. The policy applies to the
workspace members listed in the table below.

Nothing here is a promise about **1.0**. The workspace is at `0.1.0` and
under 1.0 a minor bump may break anything. What the tiers decide is
where breakage is *treated as a cost*, and where it is not.

## The tiers

| Tier | Crates | What it means |
|---|---|---|
| **Contract** | `chapbook-core`, `chapbook-paint` | Types that appear in signatures other crates must name (`LayeredLocator`, `ReadingSettings`, `Palette`, `DisplayList`). Changing one ripples through every crate below. Changed most reluctantly, and additively when possible. |
| **API** | `kalam-reader`, `chapbook-reader` | What a host calls. `kalam-reader` is the crate Kalam actually links; `chapbook-reader` is the engine session underneath it. Breaking changes are deliberate and named in the commit message. |
| **Producer** | `chapbook-epub` | The EPUB implementation of `Publication`. Kalam also depends on it directly for EPUB-specific operations, so its callers are updated with changes. |
| **Backend** | `chapbook-render-tinyskia` | Turns a `DisplayList` into pixels. The *trait* is stable; the crate implementing it is free to change. |
| **Internal** | `chapbook-layout` | No stability of any kind. Its DOM binding and cascade driver follow stylo's shape, and a stylo upgrade rewrites them. |
| **Not a library** | `kalam`, `tools/chapbook-cli` | Their surface is an application or a command line, not a Rust API. |

## Why the lines fall there

**Contract before API.** The natural first cut is "what does Kalam
call", which puts `kalam-reader` at the top. But a method added to a
session disturbs nobody, while a field added to `chapbook_core::Rect` or
a new variant of `chapbook_paint::DisplayOp` touches layout, paint and
rendering at once. Blast radius, not call frequency, earns the strictest
tier.

**`kalam-reader` is API, not Contract,** because it is the host-facing
widget boundary. The Kalam app uses it; other engine libraries do not depend
on it. A breaking change requires updating that caller in the same workspace
change.

**Internal means internal.** `chapbook-layout` — its `dom` binding, its
`cascade` driver, and layout itself — is shaped by stylo's trait
requirements, not by a design anyone chose. Promising anything about
that surface would be promising something about Servo's.

## What this asks of a change

- **Contract or API tier:** a breaking change needs a reason in the
  commit message, not just a diff. Prefer additive change; prefer a new
  method to a changed signature. Use the `kalam:` commit prefix for intentional
  engine-fork changes so they are easy to find. Upstream provenance is recorded
  in the archived `docs/kalam/UPSTREAM.md`; there is no standing update or
  comparison process.
- **Producer or Backend tier:** break freely, fix the callers in the
  same commit. The workspace builds together, so the compiler finds them.
- **Internal tier:** no ceremony. These follow the engine.

## Depending on the engine

Kalam uses `kalam-reader` and also names `chapbook-core`, `chapbook-epub`
and `chapbook-layout` directly for app integration. These are local workspace
crates; the root `Cargo.toml` uses path dependencies. No Chapbook crate is
published to crates.io. This document records the change-cost tiers for the
local engine interfaces.

The `stability` test in the CLI crate keeps the table above in step with
the workspace: every member must be named here, and nothing named here
may have left.
