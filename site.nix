# site.nix — Top-level site composition.
#
# Orchestrates the three-phase build pipeline:
#   1. Compile: render each post independently via mkPost
#   2. Link:    resolve cross-references using a collected link registry
#   3. Compose: wrap fragments in site chrome, generate index/archive pages
#
# Build with: nix-build site.nix

{ pkgs ? import <nixpkgs> {} }:

let
  # -------------------------------------------------------------------------
  # Phase 0: Build the Rust binary as a Nix derivation
  # -------------------------------------------------------------------------
  # Filter source to only Rust build files — content/ and theme/ changes
  # must NOT invalidate the binary (that would defeat per-post caching).
  rustSrc = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      let baseName = builtins.baseNameOf path;
      in
        baseName == "Cargo.toml" ||
        baseName == "Cargo.lock" ||
        pkgs.lib.hasPrefix (toString ./src) path;
  };

  post2html = pkgs.rustPlatform.buildRustPackage {
    pname = "post2html";
    version = "0.1.0";
    src = rustSrc;
    cargoLock.lockFile = ./Cargo.lock;
  };

  # -------------------------------------------------------------------------
  # Import the shared mkPost function
  # -------------------------------------------------------------------------
  mkPost = import ./lib/mkPost.nix { inherit pkgs post2html; };

  # -------------------------------------------------------------------------
  # Phase 1: Discover content directories and compile each post
  # -------------------------------------------------------------------------

  contentDir = ./content;

  # Read content/ and filter for subdirs that contain meta.nix.
  contentEntries = builtins.readDir contentDir;
  postDirNames = builtins.filter
    (name:
      contentEntries.${name} == "directory"
      && builtins.pathExists (contentDir + "/${name}/meta.nix"))
    (builtins.attrNames contentEntries);

  # Compile each post: { meta; compiled; }
  posts = map (name: mkPost (contentDir + "/${name}")) postDirNames;

  # -------------------------------------------------------------------------
  # Phase 2a: Collect metadata into a link registry (links.json)
  # -------------------------------------------------------------------------

  # Build the links attrset: slug -> { title, url }
  linksAttrs = builtins.listToAttrs (map (p: {
    name = p.meta.slug;
    value = {
      title = p.meta.title;
      url = "/posts/${p.meta.slug}/";
    };
  }) posts);

  # Validate slug uniqueness: if any slugs collide, listToAttrs silently
  # deduplicates. Compare the count of input posts vs output attr names.
  #
  # Expected error on duplicate slugs:
  #   error: Duplicate slugs detected: N posts but only M unique slugs
  #
  # This cannot be tested in the normal e2e test (which expects success).
  # To manually verify: create two content dirs with the same slug in meta.nix,
  # then run `nix-build site.nix` and observe the assertion failure.
  slugCount = builtins.length (builtins.attrNames linksAttrs);
  postCount = builtins.length posts;
  _slugCheck = if slugCount != postCount
    then builtins.throw "Duplicate slugs detected: ${toString postCount} posts but only ${toString slugCount} unique slugs"
    else true;

  linksJson = pkgs.writeText "links.json" (builtins.toJSON linksAttrs);

  # -------------------------------------------------------------------------
  # Nav link validation: every nav URL must point to a known page
  # -------------------------------------------------------------------------

  nav = [
    { label = "Home"; url = "/"; }
    { label = "Archive"; url = "/archive/"; }
    { label = "About"; url = "/posts/about/"; }
  ];

  # Build the set of all known page URLs.
  knownUrls = [ "/" "/archive/" ]
    ++ map (p: "/posts/${p.meta.slug}/") posts;

  knownUrlSet = builtins.listToAttrs
    (map (url: { name = url; value = true; }) knownUrls);

  validateNavItem = item:
    if knownUrlSet ? ${item.url}
    then true
    else builtins.throw
      "Nav link '${item.label}' points to '${item.url}' which does not exist. Known pages: ${builtins.concatStringsSep ", " knownUrls}";

  _navCheck = builtins.all (x: x) (map validateNavItem nav);

  # -------------------------------------------------------------------------
  # Phase 2b: Collect compiled posts into a single directory tree
  # -------------------------------------------------------------------------

  # Create a directory with one subdirectory per slug, each pointing to
  # the compiled derivation output.
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
  # Phase 3a: Site config
  # -------------------------------------------------------------------------

  siteConfig = pkgs.writeText "site-config.json" (builtins.toJSON {
    site_name = "Nixsite Blog";
    base_url = "https://example.com";
    language = "en";
    posts_per_page = 10;
    inherit nav;
    feed = {
      enable = true;
      title = "Nixsite Blog";
      description = "A Nix-native static site.";
    };
    author = {
      name = "mpedersen";
      email = "mp@example.com";
    };
  });

  # -------------------------------------------------------------------------
  # Phase 3b: Compose — assemble the final site
  # -------------------------------------------------------------------------

  themeDir = ./themes/default;

  site = pkgs.runCommand "site" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html compose \
      --config ${siteConfig} \
      --posts-dir ${linkedPostsDir} \
      --template-dir ${themeDir}/templates \
      --static-dir ${themeDir}/static \
      --out $out
  '';

  # Convenience aliases
  inherit (builtins) map;

in
  # Force the slug uniqueness check and nav validation to evaluate.
  assert _slugCheck;
  assert _navCheck;
  site
