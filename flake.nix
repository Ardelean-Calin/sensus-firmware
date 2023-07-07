{
  description = "Sensus development environment flake";

  inputs = {
      nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
  let
      system = "x86_64-linux";
      pkgs = import nixpkgs { 
        system = "x86_64-linux"; 
        config = { 
          allowUnfree = true;
          segger-jlink.acceptLicense = true;
        };
      };
  in
  {
    devShells.${system}.default =
      pkgs.mkShell {
          buildInputs = [
            pkgs.rustup
            pkgs.cargo-binutils
            pkgs.rustc.llvmPackages.llvm
            pkgs.nrf-command-line-tools
            pkgs.probe-run
          ]; 
          shellHook = ''
            echo "hello mom"
          '';
      };
      commands = [
        {
          name = "buildHex";
          category = "build";
          command = "cargo objcopy --release -- -O ihex app.hex";
        }
      ];
    };
}
