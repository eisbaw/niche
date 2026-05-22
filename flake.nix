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
            for f in index.html feed.xml archive/index.html \
                     posts/hello-world/index.html \
                     posts/second-post/index.html \
                     static/css/main.css; do
              test -f "$site/$f" || { echo "missing: $f"; exit 1; }
            done
            # Wiki-link resolved both directions
            grep -q "/posts/second-post/" "$site/posts/hello-world/index.html"
            grep -q "/posts/hello-world/" "$site/posts/second-post/index.html"
            # Site name from site-config.nix made it into the chrome
            grep -q "Niche Test Site" "$site/index.html"
            touch $out
          '';
        });
    };
}
