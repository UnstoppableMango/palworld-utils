{ pkgs }:
let
  # deafdudecomputers/PalworldSaveTools, pinned to a specific commit for
  # reproducibility. `make gen-palworld-types` regenerates
  # src/palworld_type_hints.rs from this file; bump rev/hash together if
  # upstream changes PALWORLD_TYPE_HINTS (see src/palworld_types.rs for what
  # consumes the generated table).
  rev = "e6cd688813999f3851e3d16e1e643bfa93865904";
  paltypesSrc = pkgs.fetchFromGitHub {
    owner = "deafdudecomputers";
    repo = "PalworldSaveTools";
    inherit rev;
    sparseCheckout = [ "src/palsav/palsav/paltypes.py" ];
    hash = "sha256-DB7T479PYSwyFCaY5EXfMgoHJJv2YODSymkkkVYJ3ZA=";
  };
in
pkgs.runCommand "palworld-type-hints-rs" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  python3 ${./gen-palworld-types.py} ${paltypesSrc}/src/palsav/palsav/paltypes.py > $out
''
