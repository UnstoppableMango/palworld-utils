{
  craneLib,
  version,
}:
craneLib.buildPackage {
  pname = "palutil";
  inherit version;

  src = craneLib.cleanCargoSource ../.;
}
