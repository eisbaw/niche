# mkPostBatch.nix — Batch compile multiple posts in a single derivation.
#
# Reduces Nix sandbox overhead by rendering N posts per derivation instead of 1.
# Tradeoff: changing 1 post rebuilds its entire batch.
#
# Takes { pkgs, post2html } at import time, returns a function:
#   postDirs -> [{ meta; slug; batchDrv; }]
#
# Each batchDrv output contains subdirectories named by slug, each with
# content.html, computed.json, and optional assets/.

{ pkgs, post2html }:

let
  # Resolve meta + content file for a single post dir (pure Nix, no derivation)
  resolvePost = postDir:
    let
      meta = import (postDir + "/meta.nix");
      contentFile =
        if builtins.pathExists (postDir + "/post.md") then postDir + "/post.md"
        else if builtins.pathExists (postDir + "/post.rst") then postDir + "/post.rst"
        else if builtins.pathExists (postDir + "/post.html") then postDir + "/post.html"
        else if builtins.pathExists (postDir + "/post.txt") then postDir + "/post.txt"
        else builtins.throw "No content file found in ${toString postDir}";
      hasAssets = builtins.pathExists (postDir + "/assets");
    in {
      inherit meta contentFile postDir hasAssets;
      configFile = pkgs.writeText "post-config-${meta.slug}.json" (builtins.toJSON meta);
    };

  # Split a list into chunks of size n
  chunksOf = n: list:
    let
      len = builtins.length list;
      go = i:
        if i >= len then []
        else [ (pkgs.lib.sublist i n list) ] ++ go (i + n);
    in go 0;

  batchSize = 50;

in

postDirs:

let
  resolved = map resolvePost postDirs;
  batches = chunksOf batchSize resolved;

  # Build one derivation per batch
  buildBatch = batchIndex: batch:
    let
      drv = pkgs.runCommand "post-batch-${toString batchIndex}" {} (
        ''
          mkdir -p $out
        '' + builtins.concatStringsSep "\n" (map (p:
          ''
            mkdir -p $out/${p.meta.slug}
            ${post2html}/bin/post2html render \
              --config ${p.configFile} \
              --content ${p.contentFile} \
              --out $out/${p.meta.slug}
            ${pkgs.lib.optionalString p.hasAssets
              "cp -r ${p.postDir + "/assets"} $out/${p.meta.slug}/assets"}
          ''
        ) batch)
      );
    in
      map (p: {
        inherit (p) meta;
        slug = p.meta.slug;
        batchDrv = drv;
      }) batch;

  allPosts = builtins.concatLists (pkgs.lib.imap0 buildBatch batches);

in allPosts
