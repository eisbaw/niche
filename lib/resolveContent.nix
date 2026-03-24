# resolveContent.nix — Find the content file in a post directory.
#
# Returns the path to the first matching content file by extension priority:
#   .md > .rst > .html > .txt
#
# Throws if no content file is found.

postDir:

if builtins.pathExists (postDir + "/post.md") then postDir + "/post.md"
else if builtins.pathExists (postDir + "/post.rst") then postDir + "/post.rst"
else if builtins.pathExists (postDir + "/post.html") then postDir + "/post.html"
else if builtins.pathExists (postDir + "/post.txt") then postDir + "/post.txt"
else builtins.throw "No content file found in ${toString postDir}"
