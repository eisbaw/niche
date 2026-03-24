---
id: TASK-0008
title: 'Remove batched build, extract shared content-resolution helper'
status: Done
assignee: []
created_date: '2026-03-24 07:07'
updated_date: '2026-03-24 07:08'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
MPED review recommends fine-grained (1 derivation per post) over batched (50 per derivation). Cold build difference is 12s vs 3.6s but incremental (the common case) is identical at 2.3s. Batched adds accidental complexity: duplicated content-resolution logic, leaky batchDrv/slug abstraction, and loss of per-post composability.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Delete site-batched.nix and lib/mkPostBatch.nix
- [ ] #2 Extract content-file resolution chain (.md > .rst > .html > .txt) from mkPost.nix into lib/resolveContent.nix
- [ ] #3 mkPost.nix imports resolveContent.nix instead of inline logic
- [ ] #4 nix-build site.nix still works
- [ ] #5 All tests pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Why fine-grained wins over batched:
1. Incremental builds identical (2.3s) — cold build diff (8.7s) does not matter in practice
2. Batched duplicates content-resolution logic — DRY violation, will diverge
3. Batched leaks abstraction — consumers must know batchDrv/slug addressing
4. Fine-grained is idiomatic Nix — one derivation per unit, overrideable, binary-cache friendly
5. Fine-grained is strictly more expressive — can always batch later, cannot un-batch

The one improvement: extract content-file resolution into a shared helper so the logic exists once.
<!-- SECTION:PLAN:END -->
