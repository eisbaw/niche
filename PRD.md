# PRD: A Nix-Native Static Site Generator

> **Historical design document.** Written when this lived as a monorepo
> together with an instance's `content/`. The repo has since been split:
> engine (this repo) exposes `lib.mkSite` via `flake.nix`; instances pin
> niche and supply their own content. Some path references and workflow
> steps in this doc no longer match the current layout — treat it as
> design rationale, not as a current operations manual. The current API
> contract is in [`README.md`](./README.md).

## Vision

A static site generator with clean, minimal aesthetics, built from scratch around Nix as the orchestration and composition layer and Rust as a nix-agnostic content processing engine.

No dynamic serving. No admin dashboard. No database. No custom config language. Nix is the config language.

## Non-Goals

- Dynamic content editing or live preview server (out of scope)
- User authentication, sessions, or multi-user workflows
- SaaS hosting or managed service (also prohibited by upstream license)
- PHP, JavaScript build pipelines, or Node.js dependencies in the core
- Inventing yet another config/template DSL — Nix already exists
- The Rust binary knowing anything about Nix

## Architecture Overview

### Key Insight: Two-Layer Design

The system is split into two layers with a clean interface between them:

1. **Rust binary (`post2html`)** — a nix-agnostic content processor. Takes a JSON config + content file on stdin/args, produces an HTML content fragment + JSON with computed fields (word count, reading time). Knows nothing about Nix, site structure, or site chrome (nav, footer). Could be driven by a Makefile, a shell script, or anything else.

2. **Nix expressions** — the orchestration layer. A shared `mkPost` function invokes the Rust binary for each post. The top-level `site.nix` collects all post outputs, resolves cross-links, wraps fragments in site chrome, and composes the final site (index, archive, tag pages, feed, etc.).

### Directory Layout

```
blog/
  site.nix              # Top-level site composition (imports all posts, assembles site)
  lib/
    mkPost.nix           # Shared function: meta.nix + content -> compiled post derivation
  themes/
    default/
      templates/
        post.html         # Full page template (site chrome + content fragment)
        index.html        # Post listing with pagination
        archive.html      # Chronological archive
        tag.html          # Posts filtered by tag
        feed.xml          # Atom/RSS feed
      static/            # CSS, fonts, images
  content/
    2024-03-nix-tricks/
      meta.nix            # Post metadata (pure Nix attrset)
      post.md             # Content (markdown)
      assets/             # Post-local images, files
    2024-04-rust-ffi/
      meta.nix
      post.rst            # Content (reStructuredText)
      assets/
  result/                # Nix build output (symlink)
```

### Data Flow (Compile + Link, like cc + ld)

The build follows a **compiler toolchain** model:

1. **Compile** — each post renders independently, leaving cross-links as unresolved placeholders
2. **Link** — a single pass resolves all placeholders using collected metadata
3. **Compose** — aggregate pages (index, archive, tags, feed) are generated

```
  Compile phase (independent, parallel — each post is its own derivation):

    mkPost reads content/nix-tricks/meta.nix   mkPost reads content/rust-ffi/meta.nix
        |                                           |
        | post2html render                          | post2html render
        | (no knowledge of other posts)             | (no knowledge of other posts)
        | (no site chrome — fragment only)           | (no site chrome — fragment only)
        v                                           v
    $out/content.html  (HTML fragment with      $out/content.html
     + computed.json    [[rust-ffi]] as           + computed.json
     + assets/          unresolved placeholder)   + assets/

    computed.json contains ONLY fields the Rust binary computes:
      { "word_count": 1842, "reading_time_minutes": 8 }
    Canonical metadata stays in Nix (meta.nix) — single source of truth.


  Link phase (single derivation — needs all compiled posts + all metadata):

    site.nix
        |
        | collects all meta.nix attrsets into link registry
        | collects all compiled post outputs
        |
        v
    post2html link --links links.json --posts-dir compiled-posts/ --out linked-posts/
        |
        | resolves [[slug]] placeholders -> <a href="...">title</a>
        | warns on stderr for broken links
        |
        +---> linked-posts/nix-tricks/content.html  (cross-links resolved)
        +---> linked-posts/rust-ffi/content.html


  Compose phase (single derivation — wraps fragments in site chrome):

    post2html compose --config site-config.json --posts-dir linked-posts/
        |
        | wraps each content.html in post.html template (nav, footer, <head>)
        | generates aggregate pages using site-wide context
        |
        +---> $out/posts/*/index.html (full post pages with site chrome)
        +---> $out/index.html         (paginated front page)
        +---> $out/archive/index.html
        +---> $out/tags/*/index.html
        +---> $out/feed.xml
        +---> $out/static/            (theme assets)
```

**Why this scales:** Adding or renaming a post does not invalidate any other post's compiled derivation — compile depends only on content + meta.nix, not on theme or other posts. Only the link and compose phases re-run, and those are fast (string replacement + template wrapping, no content parsing).

## Rust Binary (`post2html`)

### Design Principle

The binary is a **pure function**: config + content in, HTML + JSON out. It has no opinion about how it is invoked. It does not shell out to `nix`. It does not read `site.nix`. It does not walk directories looking for posts. It processes what it is given.

This means:
- Testable with plain JSON fixtures, no Nix required
- Usable outside Nix (Makefile, CI script, etc.)
- Each invocation is hermetic and parallelizable

### Subcommands

#### `post2html render` — Compile a Single Post to HTML Fragment

```
post2html render \
  --config post-config.json \
  --content post.md \
  --out ./output/
```

No `--template-dir` — render produces a content fragment, not a full page. Site chrome (nav, footer, `<head>`) is applied later by compose.

**Input: `post-config.json`**

```json
{
  "slug": "nix-tricks",
  "title": "Understanding Nix Flakes",
  "date": "2024-03-15",
  "tags": ["nix", "flakes", "packaging"],
  "summary": "A practical guide to Nix flakes for working developers.",
  "authors": ["mpedersen"],
  "series": "nix-fundamentals",
  "part": 2,
  "math": true
}
```

Any keys beyond the few required ones (`slug`, `title`, `date`) are passed through unchanged into `computed.json` for downstream use.

**Output:**

- `output/content.html` — HTML fragment with `[[wiki-links]]` left as **unresolved placeholders** (a stable, greppable marker — e.g. `<a class="wikilink" data-slug="rust-ffi">[[rust-ffi]]</a>`). This is a `<div>` of rendered content, not a full `<html>` document.
- `output/computed.json` — **only fields the Rust binary computes**, merged with passthrough config:

```json
{
  "slug": "nix-tricks",
  "title": "Understanding Nix Flakes",
  "date": "2024-03-15",
  "tags": ["nix", "flakes", "packaging"],
  "summary": "A practical guide to Nix flakes for working developers.",
  "authors": ["mpedersen"],
  "series": "nix-fundamentals",
  "part": 2,
  "math": true,
  "word_count": 1842,
  "reading_time_minutes": 8
}
```

The canonical metadata lives in `meta.nix` (Nix is the single source of truth). `computed.json` is the config passthrough plus computed fields — compose reads it so it doesn't need to re-parse content.

#### `post2html link` — Resolve Cross-References

The linker. Takes compiled posts + a link registry, resolves all `[[wiki-link]]` placeholders in the HTML.

```
post2html link \
  --links links.json \
  --posts-dir ./compiled-posts/ \
  --out ./linked-posts/
```

**Input: `links.json`**

A registry of all known posts, built by Nix from collected metadata:

```json
{
  "nix-tricks": {"title": "Understanding Nix Flakes", "url": "/posts/nix-tricks/"},
  "rust-ffi":   {"title": "Rust FFI Patterns",        "url": "/posts/rust-ffi/"}
}
```

**Cross-link syntax** (Obsidian-compatible, recognized during `render`, resolved during `link`):

| Syntax | Placeholder after `render` | Resolved after `link` |
|--------|---------------------------|----------------------|
| `[[nix-tricks]]` | `<a class="wikilink" data-slug="nix-tricks">[[nix-tricks]]</a>` | `<a href="/posts/nix-tricks/">Understanding Nix Flakes</a>` |
| `[[nix-tricks\|my text]]` | `<a class="wikilink" data-slug="nix-tricks">my text</a>` | `<a href="/posts/nix-tricks/">my text</a>` |

Unresolved links (slug not in registry) keep the `broken-link` CSS class and emit a warning on stderr. This is a **link error**, analogous to an undefined symbol in `ld`.

**Output:** copies of each post's HTML with placeholders resolved. Everything else (computed.json, assets) is copied through unchanged.

#### `post2html compose` — Assemble the Full Site

```
post2html compose \
  --config site-config.json \
  --posts-dir ./linked-posts/ \
  --template-dir themes/default/templates/ \
  --static-dir themes/default/static/ \
  --out ./site-output/
```

**Input: `site-config.json`**

```json
{
  "site_name": "mpedersen's blog",
  "base_url": "https://example.com",
  "language": "en",
  "posts_per_page": 10,
  "nav": [
    {"label": "Home", "url": "/"},
    {"label": "Archive", "url": "/archive/"},
    {"label": "About", "url": "/about/"}
  ],
  "feed": {
    "enable": true,
    "title": "mpedersen's blog",
    "description": "Notes on Nix, Rust, and systems thinking."
  },
  "author": {
    "name": "Mark Pedersen",
    "email": "mp@example.com"
  }
}
```

**Input: `linked-posts/`** — a directory where each subdirectory is a linked post (output of `post2html link`):

```
linked-posts/
  nix-tricks/
    content.html      # HTML fragment, cross-links resolved
    computed.json      # Metadata + computed fields
    assets/
  rust-ffi/
    content.html
    computed.json
    assets/
```

**Output:** the complete static site under `--out`. Compose wraps each `content.html` in the `post.html` template (adding site chrome: nav, footer, `<head>`, OpenGraph tags), then generates aggregate pages (index, archive, tags, feed).

### Content Formats

| Format | Extension | Parser | Priority |
|--------|-----------|--------|----------|
| Markdown (CommonMark + extensions) | `.md` | `comrak` | Primary |
| reStructuredText | `.rst` | Shell out to `rst2html5` | Secondary |
| Raw HTML | `.html` | Passthrough | Escape hatch |
| Plain text | `.txt` | Wrap in `<pre>` | Minimal |

Format detected by file extension. The `--content` flag takes exactly one file.

For RST: the binary shells out to `rst2html5`. Whoever invokes the binary (Nix, a script, etc.) is responsible for making `rst2html5` available on `$PATH`.

### Dependencies (Rust Crates)

| Crate | Purpose |
|-------|---------|
| `serde`, `serde_json` | JSON deserialization of config |
| `tera` | Template rendering |
| `comrak` | CommonMark Markdown parsing (GFM extensions) |
| `syntect` | Syntax highlighting for code blocks |
| `clap` | CLI argument parsing |

No `rayon` needed — parallelism happens at the Nix layer (each post derivation builds independently).

## Nix Layer

### Post `meta.nix`

Each post directory contains a `meta.nix` — a pure Nix attrset. No function arguments, no derivation, no boilerplate. This is the single source of truth for the post's metadata.

```nix
# content/nix-tricks/meta.nix
{
  slug = "nix-tricks";
  title = "Understanding Nix Flakes";
  date = "2024-03-15";
  tags = [ "nix" "flakes" "packaging" ];
  summary = "A practical guide to Nix flakes for working developers.";
  authors = [ "mpedersen" ];
  series = "nix-fundamentals";
  part = 2;
  math = true;
}
```

### `mkPost.nix` — Shared Build Function

A single function that knows how to compile any post. Eliminates per-post boilerplate — the only per-post file is `meta.nix` (and the content itself).

```nix
# lib/mkPost.nix
{ pkgs, post2html }:

# postDir: path to a content/slug/ directory containing meta.nix + content file
postDir:

let
  meta = import (postDir + "/meta.nix");

  # Find the content file (first match wins)
  contentFile =
    if builtins.pathExists (postDir + "/post.md") then postDir + "/post.md"
    else if builtins.pathExists (postDir + "/post.rst") then postDir + "/post.rst"
    else if builtins.pathExists (postDir + "/post.html") then postDir + "/post.html"
    else if builtins.pathExists (postDir + "/post.txt") then postDir + "/post.txt"
    else builtins.throw "No content file found in ${toString postDir}";

  configFile = pkgs.writeText "post-config-${meta.slug}.json" (builtins.toJSON meta);

in
{
  inherit meta;

  compiled = pkgs.runCommand "post-${meta.slug}" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html render \
      --config ${configFile} \
      --content ${contentFile} \
      --out $out
    ${pkgs.lib.optionalString (builtins.pathExists (postDir + "/assets"))
      "cp -r ${postDir + "/assets"} $out/assets"}
  '';
}
```

Note: JSON payloads are written via `pkgs.writeText` to Nix store paths — never interpolated into shell strings. This avoids shell escaping bugs with titles containing quotes, backslashes, or `$`.

### Site `site.nix`

The top-level expression orchestrates compile + link + compose:

1. **Compile**: Use `mkPost` on each content directory (parallel, independent derivations)
2. **Link**: Build link registry from collected metadata, resolve cross-references
3. **Compose**: Wrap fragments in site chrome, generate aggregate pages

```nix
{ pkgs ? import <nixpkgs> {} }:

let
  post2html = /* the built Rust binary */;
  mkPost = import ./lib/mkPost.nix { inherit pkgs post2html; };
  theme = ./themes/default;  # contains templates/ and static/

  # Discover and compile every post (parallel derivations via mkPost)
  postDirs = builtins.filter
    (d: builtins.pathExists (./content + "/${d}/meta.nix"))
    (builtins.attrNames (builtins.readDir ./content));

  posts = map (dir: mkPost (./content + "/${dir}")) postDirs;

  # Validate: slug uniqueness (hard error on collision)
  slugs = map (p: p.meta.slug) posts;
  # (assertion TBD — fail if any slug appears more than once)

  # Collect compiled outputs into one directory
  collectedCompiled = pkgs.symlinkJoin {
    name = "compiled-posts";
    paths = map (p:
      pkgs.runCommand "wrap-${p.meta.slug}" {} ''
        mkdir -p $out/${p.meta.slug}
        ln -s ${p.compiled}/* $out/${p.meta.slug}/
      ''
    ) posts;
  };

  # Build link registry from pure metadata (no derivations needed for this)
  allMeta = map (p: p.meta) posts;
  linksFile = pkgs.writeText "links.json" (builtins.toJSON (
    builtins.listToAttrs (map (m: {
      name = m.slug;
      value = { title = m.title; url = "/posts/${m.slug}/"; };
    }) allMeta)
  ));

  # Link: resolve [[wiki-links]] across all compiled posts
  linkedPosts = pkgs.runCommand "linked-posts" {} ''
    ${post2html}/bin/post2html link \
      --links ${linksFile} \
      --posts-dir ${collectedCompiled} \
      --out $out
  '';

  # Site config
  siteConfigFile = pkgs.writeText "site-config.json" (builtins.toJSON {
    site_name = "mpedersen's blog";
    base_url = "https://example.com";
    language = "en";
    posts_per_page = 10;
    nav = [ /* ... */ ];
    feed = { /* ... */ };
    author = { /* ... */ };
  });

in
# Compose: wrap fragments in site chrome + generate aggregate pages
pkgs.runCommand "site" {} ''
  ${post2html}/bin/post2html compose \
    --config ${siteConfigFile} \
    --posts-dir ${linkedPosts} \
    --template-dir ${theme}/templates \
    --static-dir ${theme}/static \
    --out $out
''
```

### Why This Split Matters (cc + ld analogy)

- **Compile scales** — each post derivation depends only on its own content + meta.nix. No theme dependency, no knowledge of other posts. Adding a post or changing site chrome does not invalidate any compiled post's Nix store path.
- **Link is cheap** — string replacement over already-rendered HTML. No parsing, no template rendering. Fast even for thousands of posts.
- **Compose owns site chrome** — nav, footer, `<head>`, OpenGraph tags are applied here, not in compile. Changing the theme only re-runs link + compose, not 500 post compilations.
- **Nix provides caching for free** — each compiled post is a derivation with a store path. Unchanged posts are never rebuilt.
- **Parallelism for free** — `nix-build` builds post derivations in parallel.
- **No per-post boilerplate** — `mkPost.nix` is the shared build function. Each post contributes only `meta.nix` + content.
- **Single source of truth** — metadata lives in `meta.nix`. The Rust binary's `computed.json` only adds derived fields (word count, reading time). No duplication.
- **No shell injection** — all JSON payloads written via `pkgs.writeText`, never interpolated into shell strings.
- **Testability** — test each subcommand independently with plain JSON + HTML files. Test the Nix expressions with `nix eval`. Test the full pipeline with `nix-build`.

## Templating

### Engine

Tera (Rust, Jinja2-like syntax). Chosen because:

- Mature Rust crate, well-maintained
- Familiar syntax for anyone who's used Jinja2, Django templates, or Ansible
- Supports template inheritance, macros, includes, filters
- No runtime JS dependency

### Template Structure (Default Theme)

Templates are only used by `compose` — `render` produces raw HTML fragments, no templates involved.

```
themes/default/
  templates/
    base.html           # Root layout: <html>, <head>, nav, footer
    post.html           # Single post page: extends base, receives content fragment
    index.html          # Post listing with pagination: extends base
    archive.html        # Chronological archive: extends base
    tag.html            # Posts filtered by tag: extends base
    feed.xml            # Atom/RSS feed (no base)
  static/
    css/
      main.css          # Minimal, modern CSS (no framework)
      code.css          # Syntax highlighting
    fonts/              # Self-hosted (Inter, JetBrains Mono or similar)
```

### Template Variables

All templates are rendered during compose, so they all have access to site-wide context:

```
site.*              — everything from site-config.json
posts[]             — list of post metadata (from each computed.json)
pagination.*        — current page, total pages, next/prev URLs
tags{}              — map of tag -> post count
current_url         — URL of the page being rendered
```

For `post.html` (single post page), additionally:

```
post.content        — the rendered HTML fragment (from content.html)
post.title, post.date, post.tags, post.summary, ...  — from computed.json
post.word_count, post.reading_time_minutes            — computed by render
```

### Design Specification (Default Theme)

Concrete values for a clean, minimal static blog.

#### Design Tokens (CSS Custom Properties)

```css
:root {
  /* Typography */
  --font-body: "Inter", system-ui, -apple-system, sans-serif;
  --font-code: "JetBrains Mono", ui-monospace, "Cascadia Code", monospace;
  --font-size-base: 1rem;                  /* 16px */
  --font-size-sm: 0.875rem;               /* 14px — captions, meta */
  --font-size-lg: 1.25rem;                /* 20px — lead paragraphs, quotes */
  --font-size-xl: 1.5em;                  /* large paragraphs */
  --line-height-body: 1.6;
  --line-height-heading: 1.3;
  --line-height-tight: 1.4;               /* quotes, large text */
  --font-weight-normal: 400;
  --font-weight-semibold: 600;
  --font-weight-bold: 750;                /* table headings, callout titles */

  /* Heading scale */
  --font-size-h1: 2rem;
  --font-size-h2: 1.5rem;
  --font-size-h3: 1.25rem;
  --font-size-h4: 1.1rem;

  /* Layout */
  --content-max-width: 50rem;              /* ~800px — main content column */
  --container-padding: 2rem;
  --block-margin: 1.5em;                   /* vertical space between blocks */
  --card-padding: 1.25rem;

  /* Spacing scale */
  --space-xs: 0.5rem;
  --space-sm: 0.75rem;
  --space-md: 1rem;
  --space-lg: 1.5rem;
  --space-xl: 2rem;
  --space-2xl: 2.5rem;

  /* Colors — light mode */
  --color-text: #1a1a1a;
  --color-text-muted: #666;
  --color-bg: #fff;
  --color-bg-subtle: rgba(128, 128, 128, 0.03);  /* input backgrounds */
  --color-border: rgba(128, 128, 128, 0.2);
  --color-accent: #2563eb;                /* links, active states */
  --color-accent-hover: #1d4ed8;
  --color-error: #ff4500dd;
  --color-code-bg: #f5f5f5;

  /* Borders */
  --border-radius: 5px;
  --border-width: 2px;

  /* Transitions */
  --transition-speed: 0.2s;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-text: #e5e5e5;
    --color-text-muted: #999;
    --color-bg: #1a1a1a;
    --color-bg-subtle: rgba(128, 128, 128, 0.08);
    --color-border: rgba(128, 128, 128, 0.25);
    --color-accent: #60a5fa;
    --color-accent-hover: #93bbfd;
    --color-code-bg: #2a2a2a;
  }
}
```

#### Supported Content Elements

We support a blog-relevant set of block types. The `render` subcommand converts Markdown/RST into HTML using these elements:

| Element | HTML output from `render` | CSS class |
|---------|--------------------------|-----------|
| Paragraph | `<p>` | — |
| Heading (h1-h6) | `<h1 id="slug">` ... `<h6>` | — |
| Blockquote | `<blockquote>` with optional `<figcaption>` | `.quote` |
| Code block | `<pre><code class="language-xxx">` | `.code-block` (syntax highlighted by syntect) |
| Inline code | `<code>` | — |
| Image | `<figure><img><figcaption>` | `.figure` |
| Unordered list | `<ul><li>` | — |
| Ordered list | `<ol><li>` | — |
| Table | `<table><thead><tbody>` | `.table` |
| Horizontal rule | `<hr>` | — |
| Link | `<a href>` | — |
| Wiki-link (unresolved) | `<a class="wikilink" data-slug="...">` | `.wikilink` |
| Callout/admonition | `<aside class="callout"><header>` | `.callout` |
| Math (if `math: true`) | KaTeX-rendered `<span class="math">` | `.math` |

No gallery, slideshow, form, embed, collapsible, or button blocks. Those are CMS features — a static blog doesn't need them. If needed later, they can be added as new content elements.

#### `base.html` Skeleton

```html
<!DOCTYPE html>
<html lang="{{ site.language }}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{% block title %}{{ site.site_name }}{% endblock %}</title>

  {% if post %}
  <meta name="description" content="{{ post.summary }}">
  <meta property="og:title" content="{{ post.title }}">
  <meta property="og:description" content="{{ post.summary }}">
  <meta property="og:type" content="article">
  <meta property="og:url" content="{{ site.base_url }}{{ current_url }}">
  {% endif %}

  <link rel="canonical" href="{{ site.base_url }}{{ current_url }}">
  <link rel="alternate" type="application/atom+xml" href="{{ site.base_url }}/feed.xml"
        title="{{ site.site_name }}">
  <link rel="stylesheet" href="/static/css/main.css">
  <link rel="stylesheet" href="/static/css/code.css">
</head>
<body>

  <header class="site-header">
    <nav class="site-nav">
      <a class="site-name" href="/">{{ site.site_name }}</a>
      <ul>
        {% for item in site.nav %}
        <li><a href="{{ item.url }}"
               {% if current_url == item.url %}aria-current="page"{% endif %}
               >{{ item.label }}</a></li>
        {% endfor %}
      </ul>
    </nav>
  </header>

  <main class="content">
    {% block content %}{% endblock %}
  </main>

  <footer class="site-footer">
    <p>&copy; {{ site.author.name }}</p>
  </footer>

</body>
</html>
```

#### `post.html` Template

```html
{% extends "base.html" %}

{% block title %}{{ post.title }} — {{ site.site_name }}{% endblock %}

{% block content %}
<article class="post">
  <header class="post-header">
    <h1>{{ post.title }}</h1>
    <div class="post-meta">
      <time datetime="{{ post.date }}">{{ post.date }}</time>
      {% if post.reading_time_minutes %}
      <span class="reading-time">{{ post.reading_time_minutes }} min read</span>
      {% endif %}
      {% if post.tags %}
      <ul class="tag-list">
        {% for tag in post.tags %}
        <li><a href="/tags/{{ tag }}/">{{ tag }}</a></li>
        {% endfor %}
      </ul>
      {% endif %}
    </div>
  </header>

  <div class="post-content">
    {{ post.content | safe }}
  </div>
</article>
{% endblock %}
```

#### `index.html` Template

```html
{% extends "base.html" %}

{% block content %}
<div class="post-list">
  {% for p in posts %}
  <article class="post-summary">
    <h2><a href="/posts/{{ p.slug }}/">{{ p.title }}</a></h2>
    <div class="post-meta">
      <time datetime="{{ p.date }}">{{ p.date }}</time>
      {% if p.tags %}
      <ul class="tag-list">
        {% for tag in p.tags %}
        <li><a href="/tags/{{ tag }}/">{{ tag }}</a></li>
        {% endfor %}
      </ul>
      {% endif %}
    </div>
    {% if p.summary %}
    <p class="summary">{{ p.summary }}</p>
    {% endif %}
  </article>
  {% endfor %}
</div>

{% if pagination.total_pages > 1 %}
<nav class="pagination" aria-label="Page navigation">
  {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}">Newer</a>{% endif %}
  <span>Page {{ pagination.current }} of {{ pagination.total_pages }}</span>
  {% if pagination.next_url %}<a href="{{ pagination.next_url }}">Older</a>{% endif %}
</nav>
{% endif %}
{% endblock %}
```

#### Core CSS (`main.css`)

```css
/* === Reset === */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* === Base === */
html {
  font-family: var(--font-body);
  font-size: var(--font-size-base);
  line-height: var(--line-height-body);
  color: var(--color-text);
  background: var(--color-bg);
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

/* === Layout === */
.content {
  max-width: var(--content-max-width);
  margin: 0 auto;
  padding: var(--space-xl) var(--container-padding);
}

/* === Site Header === */
.site-header {
  border-bottom: var(--border-width) solid var(--color-border);
}

.site-nav {
  max-width: var(--content-max-width);
  margin: 0 auto;
  padding: var(--space-md) var(--container-padding);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.site-name {
  font-weight: var(--font-weight-semibold);
  text-decoration: none;
  color: var(--color-text);
}

.site-nav ul {
  display: flex;
  gap: var(--space-lg);
  list-style: none;
}

.site-nav a {
  color: var(--color-text-muted);
  text-decoration: none;
  transition: color var(--transition-speed);
}

.site-nav a:hover,
.site-nav a[aria-current="page"] {
  color: var(--color-text);
}

/* === Typography === */
h1, h2, h3, h4, h5, h6 {
  line-height: var(--line-height-heading);
  font-weight: var(--font-weight-semibold);
  margin-top: var(--space-2xl);
  margin-bottom: var(--space-sm);
}

h1 { font-size: var(--font-size-h1); }
h2 { font-size: var(--font-size-h2); }
h3 { font-size: var(--font-size-h3); }
h4 { font-size: var(--font-size-h4); }

p, ul, ol, blockquote, pre, table, figure, aside {
  margin-bottom: var(--block-margin);
}

a {
  color: var(--color-accent);
  text-decoration: underline;
  text-decoration-thickness: 1px;
  text-underline-offset: 2px;
  transition: color var(--transition-speed);
}

a:hover { color: var(--color-accent-hover); }

/* === Post === */
.post-header { margin-bottom: var(--space-xl); }
.post-header h1 { margin-top: 0; }

.post-meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-sm) var(--space-md);
  align-items: center;
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
  margin-top: var(--space-xs);
}

.tag-list {
  display: flex;
  gap: var(--space-xs);
  list-style: none;
}

.tag-list a {
  color: var(--color-text-muted);
  text-decoration: none;
  font-size: var(--font-size-sm);
}

.tag-list a:hover { color: var(--color-accent); }

/* === Post Content Elements === */
.post-content blockquote {
  border-left: var(--border-width) solid var(--color-border);
  padding-left: var(--space-lg);
  font-size: var(--font-size-lg);
  font-style: italic;
  line-height: var(--line-height-tight);
  color: var(--color-text-muted);
}

.post-content code {
  font-family: var(--font-code);
  font-size: 0.9em;
  background: var(--color-code-bg);
  padding: 0.15em 0.35em;
  border-radius: 3px;
}

.post-content pre {
  background: var(--color-code-bg);
  padding: var(--space-md);
  border-radius: var(--border-radius);
  overflow-x: auto;
  line-height: 1.5;
}

.post-content pre code {
  background: none;
  padding: 0;
  font-size: var(--font-size-sm);
}

.post-content img {
  max-width: 100%;
  height: auto;
  border-radius: var(--border-radius);
}

.post-content figure {
  text-align: center;
}

.post-content figcaption {
  font-size: var(--font-size-sm);
  color: var(--color-text-muted);
  margin-top: var(--space-xs);
}

.post-content table {
  width: 100%;
  border-collapse: collapse;
}

.post-content th,
.post-content td {
  padding: 0.55em 0.7em;
  border-bottom: 1px solid var(--color-border);
  text-align: left;
}

.post-content th {
  font-weight: var(--font-weight-bold);
}

.post-content .callout {
  border: var(--border-width) solid var(--color-border);
  border-radius: var(--border-radius);
  padding: var(--space-sm) var(--space-md);
  margin-bottom: var(--block-margin);
}

.post-content .callout header {
  font-weight: var(--font-weight-bold);
  margin-bottom: var(--space-xs);
}

.post-content ul,
.post-content ol {
  padding-left: var(--space-lg);
}

.post-content li + li {
  margin-top: 0.35em;
}

/* === Wiki-links === */
.wikilink {
  color: var(--color-accent);
  text-decoration: underline;
  text-decoration-style: dashed;
}

.wikilink.broken-link {
  color: var(--color-error);
  text-decoration: line-through;
}

/* === Post List (index) === */
.post-list { display: flex; flex-direction: column; gap: var(--space-xl); }

.post-summary h2 {
  font-size: var(--font-size-h2);
  margin: 0 0 var(--space-xs);
}

.post-summary h2 a {
  color: var(--color-text);
  text-decoration: none;
}

.post-summary h2 a:hover { color: var(--color-accent); }

.post-summary .summary {
  color: var(--color-text-muted);
  margin-top: var(--space-xs);
}

/* === Pagination === */
.pagination {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: var(--space-md);
  margin-top: var(--space-2xl);
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
}

/* === Footer === */
.site-footer {
  border-top: var(--border-width) solid var(--color-border);
  max-width: var(--content-max-width);
  margin: var(--space-2xl) auto 0;
  padding: var(--space-md) var(--container-padding);
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
}
```

#### Fonts

Self-hosted in `themes/default/static/fonts/`. Both Inter and JetBrains Mono are available under the SIL Open Font License:

- `Inter-Regular.woff2`, `Inter-SemiBold.woff2`, `Inter-Bold.woff2`
- `JetBrainsMono-Regular.woff2`

Loaded via `@font-face` at the top of `main.css` (not shown above for brevity). Use `font-display: swap` to avoid FOIT.

## Change Detection and Deployment

Change detection is handled by **git**, not by the tool. Nix's store-path hashing gives us free incremental builds at the derivation level.

Deployments are tracked via **annotated git tags** in `YYYYMMDDHHMMSS` format, pushed to remote. This gives a complete deployment history via `git tag -l` and easy rollback via `git checkout <tag>`.

## Development Setup

### Nix Environment (`shell.nix`)

```nix
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc cargo clippy rustfmt
    python3Packages.docutils   # rst2html5
    just
  ];
}
```

### Justfile Recipes

```
build       # nix-build site.nix (or nix build .#site)
clean       # rm -f result
serve       # python3 -m http.server -d result/
new <slug>  # scaffold new post directory with meta.nix + post.md
check       # cargo clippy + cargo test
fmt         # cargo fmt
e2e         # end-to-end test: nix-build sample site, diff against expected output
```

## Milestones

### M1 — Rust Binary Skeleton

- Rust project scaffolding (`cargo init`, `shell.nix`, `justfile`)
- `post2html render` subcommand: read JSON config + markdown file, produce `content.html` fragment + `computed.json`
- Testable with plain files, no Nix, no templates

### M2 — Nix Integration

- `mkPost.nix` shared build function
- `meta.nix` per post
- `site.nix` that compiles posts via `mkPost`, runs link + compose
- `post2html compose` subcommand: wrap fragments in templates, generate index page
- One template (`base.html` + `post.html`), minimal CSS
- `nix-build` produces complete site in `result/`

### M3 — Content Pipeline

- Markdown with syntax highlighting (comrak + syntect)
- reStructuredText via `rst2html5`
- Proper `computed.json` output (word count, reading time)

### M4 — Aggregate Pages

- Paginated index
- Tag pages
- Archive page
- Atom feed

### M5 — Theme and Polish

- Default theme with clean, minimal aesthetics
- Typography (Inter + JetBrains Mono, self-hosted)
- Dark mode (`prefers-color-scheme`)
- Responsive layout
- OpenGraph meta tags
- Proper `<head>` (canonical URLs, favicon, etc.)

### M6 — Hardening

- End-to-end tests (sample site with expected output)
- Edge cases: posts with no tags, empty content, unicode slugs, very long posts
- Error messages that point at the problem (file path, line number where possible)
- Documentation (README, not generated docs)

## Open Questions

1. **Image processing** — Should post2html resize/optimize images, or leave that to external tools (e.g., `imagemagick` in a Nix derivation per post)? Leaning toward external — keep the core simple, and Nix derivations per-image would cache nicely.
2. **Live reload during development** — Worth building in, or just use `entr` / `watchexec` externally? Leaning external.
3. **Search** — Client-side search index (like Pagefind)? Or out of scope for now?
4. **i18n** — Multi-language support? Probably not in M1-M6. Can be added later via Nix's composability (per-language site.nix imports).
5. ~~**`render` vs `compose` split**~~ — Resolved: `render` produces content fragments. `compose` wraps in site chrome. This keeps compiled derivations independent of theme changes.

## Constraints

- This is an original implementation; its code is independently licensed.
- RST support depends on Python's `docutils` being available on `$PATH`. The Rust binary does not bundle it.
- Nix evaluation adds latency to the first build. Subsequent builds benefit from the Nix store cache — unchanged posts are not rebuilt.
