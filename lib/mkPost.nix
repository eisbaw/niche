# mkPost.nix — Shared build function for compiling a single post.
#
# Takes { pkgs, post2html } at import time, returns a function:
#   postDir -> { meta; compiled; }
#
# postDir must contain meta.nix and a content file (post.md, post.rst, post.html, or post.txt).
# The compiled derivation runs post2html render and copies assets if present.

{ pkgs, post2html }:

postDir:

let
  meta = import (postDir + "/meta.nix");

  # Find the content file by extension priority: .md > .rst > .html > .txt
  contentFile =
    if builtins.pathExists (postDir + "/post.md") then postDir + "/post.md"
    else if builtins.pathExists (postDir + "/post.rst") then postDir + "/post.rst"
    else if builtins.pathExists (postDir + "/post.html") then postDir + "/post.html"
    else if builtins.pathExists (postDir + "/post.txt") then postDir + "/post.txt"
    else builtins.throw "No content file found in ${toString postDir}";

  # Write config JSON to the Nix store — never interpolated into shell strings.
  # This avoids shell escaping bugs with titles containing quotes, backslashes, or $.
  configFile = pkgs.writeText "post-config-${meta.slug}.json" (builtins.toJSON meta);

  hasAssets = builtins.pathExists (postDir + "/assets");

in
{
  inherit meta;

  compiled = pkgs.runCommand "post-${meta.slug}" {} ''
    mkdir -p $out
    ${post2html}/bin/post2html render \
      --config ${configFile} \
      --content ${contentFile} \
      --out $out
    ${pkgs.lib.optionalString hasAssets
      "cp -r ${postDir + "/assets"} $out/assets"}
  '';
}
