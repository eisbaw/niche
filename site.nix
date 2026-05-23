# site.nix — Build a site from content + theme + config.
#
# Orchestrates the three-phase build pipeline:
#   1. Compile: render each post independently via mkPost
#   2. Link:    resolve cross-references using a collected link registry
#   3. Compose: wrap fragments in site chrome, generate index/archive pages
#
# Invoked from a per-instance flake via niche.lib.mkSite { ... }.

{ pkgs
, post2html       # the pre-built post2html derivation
, contentDir      # path: directory of post-* subdirs, each with meta.nix
, siteConfig      # attrset serialized to site-config.json (see schema below)
, themeDir ? ./themes/default
}:

# siteConfig schema:
#   site_name      string
#   base_url       string
#   language       string
#   posts_per_page int
#   nav            [ { label; url; external ? false; } ]
#                  Items default to validated: url must be "/", "/archive/",
#                  or "/posts/<known-slug>/". Set external=true to opt out
#                  (outbound links, anchors, tag indexes, etc.).
#   feed           { enable; title; description; }
#   author         { name; email; }

let
  mkPost = import ./lib/mkPost.nix { inherit pkgs post2html; };

  # -------------------------------------------------------------------------
  # Phase 1: Discover content directories and compile each post
  # -------------------------------------------------------------------------

  contentEntries = builtins.readDir contentDir;
  postDirNames = builtins.filter
    (name:
      contentEntries.${name} == "directory"
      && builtins.pathExists (contentDir + "/${name}/meta.nix"))
    (builtins.attrNames contentEntries);

  posts = map (name: mkPost (contentDir + "/${name}")) postDirNames;

  # -------------------------------------------------------------------------
  # Phase 2a: Collect metadata into a link registry (links.json)
  # -------------------------------------------------------------------------

  linksAttrs = builtins.listToAttrs (map (p: {
    name = p.meta.slug;
    value = {
      title = p.meta.title;
      url = "/posts/${p.meta.slug}/";
    };
  }) posts);

  # listToAttrs silently dedupes; catch collisions explicitly.
  slugCount = builtins.length (builtins.attrNames linksAttrs);
  postCount = builtins.length posts;
  _slugCheck = if slugCount != postCount
    then builtins.throw "Duplicate slugs detected: ${toString postCount} posts but only ${toString slugCount} unique slugs"
    else true;

  linksJson = pkgs.writeText "links.json" (builtins.toJSON linksAttrs);

  # -------------------------------------------------------------------------
  # Nav link validation: nav URLs that target this site's known pages
  # must resolve. Items marked `external = true;` opt out (for outbound
  # links, anchors, tag indexes, or anything outside the engine's
  # routing convention).
  # -------------------------------------------------------------------------

  nav = siteConfig.nav or [];

  knownUrls = [ "/" "/archive/" ]
    ++ map (p: "/posts/${p.meta.slug}/") posts;

  knownUrlSet = builtins.listToAttrs
    (map (url: { name = url; value = true; }) knownUrls);

  validateNavItem = item:
    if (item.external or false) then true
    else if knownUrlSet ? ${item.url} then true
    else builtins.throw
      "Nav link '${item.label}' points to '${item.url}' which is not a known page. Set `external = true;` if this is intentional, or fix the URL. Known pages: ${builtins.concatStringsSep ", " knownUrls}";

  _navCheck = builtins.all (x: x) (map validateNavItem nav);

  # -------------------------------------------------------------------------
  # Phase 2b: Collect compiled posts into a single directory tree
  # -------------------------------------------------------------------------

  compiledPostsDir = pkgs.runCommand "compiled-posts" {} (
    ''
      mkdir -p $out
    '' + builtins.concatStringsSep "\n" (map (p:
      "ln -s ${p.compiled} $out/${p.meta.slug}"
    ) posts)
  );

  # -------------------------------------------------------------------------
  # Phase 2c: Link phase — resolve cross-references
  # -------------------------------------------------------------------------

  linkedPostsDir = pkgs.runCommand "linked-posts" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html link \
      --links ${linksJson} \
      --posts-dir ${compiledPostsDir} \
      --out $out
  '';

  # -------------------------------------------------------------------------
  # Phase 3a: Serialize site config to JSON
  # -------------------------------------------------------------------------

  siteConfigJson = pkgs.writeText "site-config.json" (builtins.toJSON siteConfig);

  # -------------------------------------------------------------------------
  # Phase 3b: Compose — assemble the final site
  # -------------------------------------------------------------------------

  site = pkgs.runCommand "site" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html compose \
      --config ${siteConfigJson} \
      --posts-dir ${linkedPostsDir} \
      --template-dir ${themeDir}/templates \
      --static-dir ${themeDir}/static \
      --out $out
  '';

in
  assert _slugCheck;
  assert _navCheck;
  site
