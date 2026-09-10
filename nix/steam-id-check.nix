{
  pkgs,
  palutil,
}:
let
  # hub.tcno.co's Palworld save converter JS (Wesley Pyburn / TCNOco), pinned
  # so this cross-check is reproducible. src/steam_id.rs is a from-scratch
  # Rust port of the SteamID64 -> PlayerUId algorithm in this file; bump the
  # URL/hash together if upstream changes it.
  referenceJs = pkgs.fetchurl {
    url = "https://hub.tcno.co/js/palworld-steam-id.min.cc26300785d8b0908c5a99afd9d9fac1c68b2a3e53786d6857a7ef8921fcf7ecca7669ffa342c16769631408971fdd3f260aa17cbb13caf8edde1376f124795c.js";
    hash = "sha256-73lOIaj75kIIvquLkdhLq4H1yGtdn1aqhKZxZCU+fMc=";
  };

  testIds = [
    "76561198012345678"
    "76561197960287930"
    "76561198000000000"
  ];
in
pkgs.runCommand "steam-id-cross-check"
  {
    nativeBuildInputs = [
      pkgs.nodejs
      palutil
    ];
  }
  ''
    set -euo pipefail

    node -e '
      const m = require(process.argv[1]);
      for (const id of process.argv.slice(2)) {
        console.log(id + " " + m.steamId64ToPalworldPlayerId(id));
      }
    ' ${referenceJs} ${pkgs.lib.concatStringsSep " " testIds} > js-output.txt

    for id in ${pkgs.lib.concatStringsSep " " testIds}; do
      echo "$id $(palworld-utils steam-id "$id")"
    done > rust-output.txt

    diff js-output.txt rust-output.txt

    touch $out
  ''
