---
id: TASK-0009
title: Validate nav links point to existing pages
status: Done
assignee: []
created_date: '2026-03-24 07:52'
updated_date: '2026-03-24 07:56'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
site.nix defines nav links (Home, Archive, About) but does not verify the target pages exist. The About page is missing, producing a broken nav link. site.nix should assert that all nav URLs correspond to either a generated aggregate page (/, /archive/) or an existing post. Missing targets should be a hard build failure, not a silent 404.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 site.nix validates all nav URLs resolve to real pages
- [ ] #2 Missing nav target produces clear nix-build error
- [ ] #3 Add an about page (content/about/) or remove About from nav
<!-- AC:END -->
