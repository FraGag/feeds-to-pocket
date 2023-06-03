{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    #nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    # Last commit in nixpkgs-unstable before 8ed6a3e6bb54580e8ef5ac44dd9006ea69bcdbeb, which marked openssl_1_1 as nearing EOL.
    nixpkgs.url = "github:NixOS/nixpkgs?rev=8ed6a3e6bb54580e8ef5ac44dd9006ea69bcdbeb";
  };

  outputs = { self, flake-utils, naersk, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        naersk' = pkgs.callPackage naersk { };

        buildPackage = devMode: naersk'.buildPackage {
          src = ./.;

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl_3 ];

          singleStep = devMode;
        };
      in
      {
        # For `nix build` & `nix run`:
        packages.default = buildPackage false;

        # For `nix develop`:
        devShells.default = (buildPackage true).overrideAttrs (finalAttrs: previousAttrs: {
          nativeBuildInputs = previousAttrs.nativeBuildInputs ++ (with pkgs; [
            clippy
            gitFull
            rustfmt
          ]);
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        });
      }
    );
}
