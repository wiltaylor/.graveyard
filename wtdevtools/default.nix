{pkgs}:
pkgs.python3Packages.buildPythonPackage rec {
  pname = "wtdevtools";
  version = "0.0.1";
  propagatedBuildInputs = with pkgs.python3Packages; [ 
    click
  ];

  src = builtins.path { path = ./.; name = "devtools"; };

  doCheck = false;
}

