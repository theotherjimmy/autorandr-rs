let
  pkgs = import <nixpkgs> {};
in
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    rustfmt
    cargo
    cargo-crev
    cargo-watch
    rls
    rust-analyzer
    gnuplot
    bingrep
    scdoc
    groff
  ];
}
