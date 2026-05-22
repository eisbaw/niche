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
      lib.mkSite = args@{ pkgs, ... }: import ./site.nix (args // {
        post2html = self.packages.${pkgs.system}.post2html;
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
    };
}
