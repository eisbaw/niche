---
id: TASK-0007
title: Profile build latency with hundreds of posts
status: Done
assignee: []
created_date: '2026-03-24 05:46'
updated_date: '2026-03-24 06:05'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a benchmark blog with 300+ generated posts to measure build latency across the compile/link/compose pipeline. First generate the posts (varied tags, dates, content lengths, some with wiki-links), then profile nix-build to identify bottlenecks: Nix evaluation time, per-post compile derivation time, link phase, compose phase. Report wall-clock times for each phase.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Generate 300+ sample posts with realistic variation
- [ ] #2 Measure total nix-build wall-clock time
- [ ] #3 Measure per-phase times: nix eval, compile, link, compose
- [ ] #4 Identify the bottleneck phase
- [ ] #5 Document results and any optimization opportunities
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Build Latency Profile (305 posts)

| Scenario | Time |
|----------|------|
| Full cold build (305 posts) | 1m55s |
| Warm rebuild (nothing changed) | 0.7s |
| Incremental (1 post changed) | 2.2s |
| Incremental (1 new post added) | 2.1s |
| Nix evaluation only | 3.0s |

### Before source filter fix
Incremental rebuild was 1m58s because content/ changes dirtied the Rust binary hash via `cleanSource ./.`, invalidating all 305 post derivations.

### After source filter fix
`cleanSourceWith` filters to only Cargo.toml, Cargo.lock, and src/. Content and theme changes no longer rebuild the binary. Only the changed post compiles + link + compose re-run.

### Bottleneck
Full cold build: dominated by 305 individual post compile derivations (each is a separate nix-build). Nix eval is ~3s. Link+compose is <1s. The per-derivation overhead of nix sandboxing is the cost — each post is fast but there are 305 of them.

### Optimization opportunities
- Batch multiple posts into a single derivation (trades caching granularity for speed)
- Use `--max-jobs` to increase Nix build parallelism
- Pre-build the binary and pin its store path to avoid re-evaluation
<!-- SECTION:NOTES:END -->
