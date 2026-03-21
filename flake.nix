{
  description = "pgoauth — PostgreSQL 18 OAuth extension dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable."1.84.0".default.override {
          extensions = [ "rustfmt" "clippy" "rust-src" ];
        };

        pythonEnv = pkgs.python3.withPackages (ps: [
          ps.pyjwt
          ps.cryptography
        ]);
      in
      {
        devShells.default = pkgs.mkShell {
          name = "pgoauth-dev";

          buildInputs = [
            # Rust toolchain
            rustToolchain

            # PostgreSQL 18 (dev headers + pg_config + server)
            pkgs.postgresql_18

            # C toolchain — required by pgrx's bindgen
            pkgs.clang
            pkgs.llvmPackages.libclang

            # Build dependencies for ureq (TLS) and pgrx
            pkgs.pkg-config
            pkgs.openssl
            pkgs.openssl.dev
            pkgs.zlib

            # Python for test/gen_token.py
            pythonEnv

            # Misc utilities used in Makefile / tests
            pkgs.gnumake
            pkgs.jq
            pkgs.curl
          ];

          # bindgen needs to find libclang
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          # Point pgrx at the nix-provided pg_config
          PG_CONFIG = "${pkgs.postgresql_18}/bin/pg_config";

          # openssl-sys crate
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

          shellHook = ''
            echo "pgoauth dev shell (Rust $(rustc --version | cut -d' ' -f2), PG18)"

            # Install cargo-pgrx at the exact version matching the crate (0.17.0)
            if ! cargo pgrx --version 2>/dev/null | grep -q "0\.17\."; then
              echo "Installing cargo-pgrx 0.17.0 …"
              cargo install cargo-pgrx --version 0.17.0 --locked
            fi

            # Initialise pgrx against the nix PG18 installation
            PGRX_HOME="''${PGRX_HOME:-$HOME/.pgrx}"
            if [ ! -f "$PGRX_HOME/config.toml" ]; then
              echo "Initialising pgrx with PG18 …"
              cargo pgrx init --pg18 "$PG_CONFIG"
            fi

            echo ""
            echo "  make unit-test   — Rust unit tests"
            echo "  make token       — generate a test JWT"
            echo "  make up          — start postgres + jwks (needs Docker)"
            echo "  make test        — full integration suite"
          '';
        };
      }
    );
}
