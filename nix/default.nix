{
  buildGoApplication,
  lib,
  version,
}:
buildGoApplication {
  pname = "palutil";
  inherit version;

  src = lib.cleanSource ../.;
  modules = ./gomod2nix.toml;
}
