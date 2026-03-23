# the reference CMS

A Nix-native static site generator. Content is authored as Markdown with Nix
attribute sets for metadata. The build pipeline is fully reproducible via
`nix-build`.

## Prerequisites

- [Nix](https://nixos.org/download/) (provides Rust toolchain, just, and all
  dependencies via `shell.nix`)

## Quickstart

```sh
# Build the site (output lands in result/)
nix-build site.nix

# Serve locally
nix-shell --run "just serve"
```

## Adding a post

```sh
nix-shell --run "just new my-post-slug"
```

This creates `content/my-post-slug/` with a `meta.nix` template and empty
`post.md`. Edit both files, then rebuild with `nix-build site.nix`.

### meta.nix format

```nix
{
  slug = "my-post-slug";
  title = "My Post Title";
  date = "2024-03-15";
  tags = [ "topic" ];           # optional
  summary = "Short description"; # optional
  authors = [ "name" ];          # optional
}
```

### Content formats

The content file can be `post.md` (Markdown), `post.rst` (reStructuredText),
`post.html` (passthrough), or `post.txt` (plain text wrapped in `<pre>`).

Wiki-links are supported: `[[other-slug]]` links to another post by slug.

## Directory structure

```
content/          Post source directories (one per post)
  hello-world/
    meta.nix      Post metadata
    post.md       Post content
    assets/       Optional static assets copied alongside the post
lib/              Shared Nix functions (mkPost.nix)
src/              Rust source for the post2html binary
themes/default/   Templates (Tera) and static assets (CSS, fonts)
tests/            Integration tests and e2e test script
site.nix          Top-level build: compile -> link -> compose
shell.nix         Nix development shell
justfile          Common development commands
```

## Development

```sh
nix-shell
just check        # clippy + tests
just fmt          # format Rust source
just e2e          # full end-to-end test
just site         # build via nix-build site.nix
```
