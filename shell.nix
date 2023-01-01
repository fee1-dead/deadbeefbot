{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  buildInputs = [ gcc pkg-config openssl ];  
}
