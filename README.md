# niche

A Nix-native static site engine. Posts authored in Markdown / RST / HTML /
plain text, metadata as Nix attribute sets, build orchestrated by a flake
that wraps a small Rust binary (`post2html`).

niche is the **engine**. To run a site you also need an **instance** —
a separate flake that holds your content and calls `niche.lib.mkSite`.
The reference instance lives at
[`example-instance`](../example-instance) (sibling repo).

## Prerequisites

- [Nix](https://nixos.org/download/) with flakes enabled
  (`experimental-features = nix-command flakes` in `nix.conf`).

## Public API

The flake exposes one function and one package:

```nix
# flake.nix
lib.mkSite = { pkgs, contentDir, siteConfig, themeDir ? <niche>/themes/fancy-sidebar }: derivation;
packages.<system>.post2html = derivation;   # the renderer binary
devShells.<system>.default  = derivation;   # rust + docutils + just
checks.<system>.e2e         = derivation;   # smoke-builds tests/fixtures/site
```

### `lib.mkSite` arguments

| arg | type | meaning |
|---|---|---|
| `pkgs`        | nixpkgs attrset    | Provides the build environment. niche reads `pkgs.system` from `pkgs.stdenv.hostPlatform.system` to select a matching `post2html` build. |
| `contentDir`  | path               | Directory whose subdirs are posts. Each subdir needs a `meta.nix` and one of `post.{md,rst,html,txt}`. |
| `siteConfig`  | attrset            | Serialized to JSON and consumed by the renderer (see schema below). |
| `themeDir`    | path *(optional)*  | Theme root with `templates/` (Tera) and `static/` (CSS, fonts). Defaults to the bundled `fancy-sidebar` theme; `plain` is also bundled. |

### `siteConfig` schema

```nix
{
  site_name       = "string";
  base_url        = "string";   # no trailing slash
  language        = "string";   # e.g. "en"
  posts_per_page  = 10;
  nav = [
    { label = "Home";    url = "/"; }
    { label = "Archive"; url = "/archive/"; }
    { label = "About";   url = "/posts/about/"; }
    # set external=true to skip URL validation (outbound, anchors, ...)
    { label = "Source";  url = "https://git.example/me"; external = true; }
  ];
  feed = { enable = true; title = "..."; description = "..."; };
  author = { name = "..."; email = "..."; };
}
```

Nav URLs are validated against discovered pages (`"/"`, `"/archive/"`,
and `/posts/<slug>/` for every post in `contentDir`). Items with
`external = true;` skip the check.

### `meta.nix` per post

```nix
{
  slug    = "my-post-slug";
  title   = "My Post Title";
  date    = "2026-01-15";
  tags    = [ "topic" ];          # optional
  summary = "Short description";  # optional
  authors = [ "name" ];           # optional
}
```

Wiki-links: `[[other-slug]]` in any content format renders as an
anchor whose `href` is resolved at link phase. Unresolved slugs render
as `<a class="wikilink broken-link" data-slug="...">` so they're
visually distinguishable from working links.

## Minimal instance flake

```nix
{
  inputs.niche.url = "git+file:///path/to/niche";
  inputs.nixpkgs.follows = "niche/nixpkgs";

  outputs = { self, niche, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages.${system}.default = niche.lib.mkSite {
        inherit pkgs;
        contentDir = ./content;
        siteConfig = import ./site-config.nix;
      };
    };
}
```

See `../example-instance/instances/main/flake.nix` for a complete example
that supports multiple systems.

## Self-test

```sh
just e2e         # runs nix flake check
nix flake check  # equivalent
```

Builds the fixture site under `tests/fixtures/site` and asserts on
file existence, wiki-link resolution across content formats,
broken-link rendering, feed content, and the external-nav opt-out path.

## Development

```sh
nix develop      # or: nix-shell
just check       # cargo clippy -D warnings && cargo test
just fmt         # cargo fmt
just build       # cargo build
```

The Rust binary lives under `src/`; Nix orchestration under
`lib/` and `site.nix`. Two themes ship under `themes/`: `fancy-sidebar`
(the default) and `plain`.

## Repository layout

```
src/              Rust source for the post2html binary
lib/              Shared Nix functions (mkPost.nix, resolveContent.nix)
themes/           Bundled themes: fancy-sidebar (default), plain
tests/            cargo integration tests + fixture site for flake e2e
site.nix          Build pipeline (compile → link → compose)
flake.nix         Public API: lib.mkSite, packages, devShells, checks
shell.nix         Dev shell (also re-exported as devShells.default)
PRD.md            Original design doc (historical; pre-split)
```
