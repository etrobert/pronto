{
  description = "Pronto - Rust-based shell prompt/status tool";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "pronto";
            version = self.rev or "dirty";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            meta = with pkgs.lib; {
              description = "Rust-based shell prompt/status tool";
              license = licenses.mit;
              platforms = platforms.unix;
            };
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              rust-analyzer
            ];
          };
        }
      );
    };
}
