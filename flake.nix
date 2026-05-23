{
  description = "Niche — Nix-native static site engine";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor = system: import nixpkgs { inherit system; };
    in {
      # mkSite is the primary entry point for a per-instance flake:
      #   niche.lib.mkSite { pkgs; contentDir; siteConfig; themeDir ? ...; }
      lib.mkSite = args@{ pkgs, ... }:
        if !(nixpkgs.lib.elem pkgs.stdenv.hostPlatform.system systems) then
          throw "niche: unsupported system '${pkgs.stdenv.hostPlatform.system}'. Supported: ${nixpkgs.lib.concatStringsSep ", " systems}"
        else
          import ./site.nix (args // {
            post2html = self.packages.${pkgs.stdenv.hostPlatform.system}.post2html;
          });

      packages = forAllSystems (system: {
        post2html = (pkgsFor system).rustPlatform.buildRustPackage {
          pname = "post2html";
          version = "0.1.0";
          src = (pkgsFor system).lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let baseName = builtins.baseNameOf path;
              in
                baseName == "Cargo.toml" ||
                baseName == "Cargo.lock" ||
                (pkgsFor system).lib.hasPrefix (toString ./src) path;
          };
          cargoLock.lockFile = ./Cargo.lock;
        };
        default = self.packages.${system}.post2html;
      });

      devShells = forAllSystems (system: {
        default = import ./shell.nix { pkgs = pkgsFor system; };
      });

      # `nix flake check` and `just e2e` drive this: build the fixture
      # site under tests/fixtures/site and assert key output files exist.
      checks = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          fixtureSite = self.lib.mkSite {
            inherit pkgs;
            contentDir = ./tests/fixtures/site/content;
            siteConfig = import ./tests/fixtures/site/site-config.nix;
          };
        in {
          e2e = pkgs.runCommand "niche-e2e" {} ''
            set -e
            site=${fixtureSite}

            # All four content formats land in the output. Catches the
            # high-leverage regression where compose silently restricts
            # its post glob to *.md.
            for f in index.html feed.xml archive/index.html \
                     posts/hello-world/index.html \
                     posts/second-post/index.html \
                     posts/rst-post/index.html \
                     posts/html-post/index.html \
                     posts/broken-link-post/index.html \
                     static/css/main.css; do
              test -f "$site/$f" || { echo "missing: $f"; exit 1; }
            done

            # Wiki-link resolved in both directions and across formats.
            grep -q '/posts/second-post/' "$site/posts/hello-world/index.html"
            grep -q '/posts/hello-world/' "$site/posts/second-post/index.html"
            grep -q '/posts/hello-world/' "$site/posts/rst-post/index.html"
            grep -q '/posts/hello-world/' "$site/posts/html-post/index.html"

            # HTML passthrough survived render and compose.
            grep -q 'data-marker="html-passthrough"' "$site/posts/html-post/index.html"

            # Broken wiki-link tagged as broken-link, not silently dropped.
            grep -q 'class="wikilink broken-link"' "$site/posts/broken-link-post/index.html"

            # Site config made it into HTML chrome.
            grep -q "Niche Test Site" "$site/index.html"

            # Feed has real <entry> elements with the right ids/links,
            # not a malformed empty feed.
            grep -q '<entry>' "$site/feed.xml"
            grep -q '<id>https://example.test/posts/hello-world/</id>' "$site/feed.xml"
            grep -q '<id>https://example.test/posts/second-post/</id>' "$site/feed.xml"

            # external=true nav item kept (validates the opt-out path).
            # URL slashes are HTML-entity-encoded by Tera, so match on label.
            grep -q '>Source</a>' "$site/index.html"

            # Heading demotion: each post page has exactly one <h1>
            # (the template-provided title). Post body content is h2+.
            for slug in hello-world second-post rst-post html-post broken-link-post; do
              h1_count=$(grep -oE '<h1\b' "$site/posts/$slug/index.html" | wc -l)
              test "$h1_count" -eq 1 \
                || { echo "$slug has $h1_count h1 elements, expected 1"; exit 1; }
            done

            touch $out
          '';
        });
    };
}
