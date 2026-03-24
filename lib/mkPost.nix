# mkPost.nix — Shared build function for compiling a single post.
#
# Takes { pkgs, post2html } at import time, returns a function:
#   postDir -> { meta; compiled; }
#
# postDir must contain meta.nix and a content file (post.md, post.rst, post.html, or post.txt).
# The compiled derivation runs post2html render and copies assets if present.
#
# If postDir contains a figures.nix, it is built and its SVG output is:
#   1. Passed via --assets-dir so SVGs are inlined into the HTML during render
#   2. Copied to $out/assets/ for direct access as well

{ pkgs, post2html }:

postDir:

let
  meta = import (postDir + "/meta.nix");

  # Find the content file by extension priority: .md > .rst > .html > .txt
  contentFile = import ./resolveContent.nix postDir;

  # Write config JSON to the Nix store — never interpolated into shell strings.
  configFile = pkgs.writeText "post-config-${meta.slug}.json" (builtins.toJSON meta);

  hasAssets = builtins.pathExists (postDir + "/assets");
  hasFigures = builtins.pathExists (postDir + "/figures.nix");
  figures = if hasFigures then import (postDir + "/figures.nix") { inherit pkgs; } else null;

  # If figures exist, create a merged assets directory for --assets-dir.
  # This lets render_file resolve assets/X.svg references during rendering.
  assetsDir =
    if hasFigures then
      pkgs.runCommand "assets-${meta.slug}" {} ''
        mkdir -p $out/assets
        ${pkgs.lib.optionalString hasAssets "cp -r ${postDir + "/assets"}/* $out/assets/"}
        cp -r ${figures}/* $out/assets/
      ''
    else if hasAssets then
      # Wrap the raw assets dir so --assets-dir has consistent structure
      pkgs.runCommand "assets-${meta.slug}" {} ''
        mkdir -p $out
        ln -s ${postDir + "/assets"} $out/assets
      ''
    else
      null;

  hasAssetsDir = assetsDir != null;

in
{
  inherit meta;

  compiled = pkgs.runCommand "post-${meta.slug}" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html render \
      --config ${configFile} \
      --content ${contentFile} \
      ${pkgs.lib.optionalString hasAssetsDir "--assets-dir ${assetsDir}"} \
      --out $out
    ${pkgs.lib.optionalString hasAssetsDir ''
      cp -r ${assetsDir}/assets $out/assets
    ''}
  '';
}
