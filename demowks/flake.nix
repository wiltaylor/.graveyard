{
  description = "A demo nix workspace";

  inputs = {
    wks.url = "github:wiltaylor/nixwks";
  };

  outputs = { self, nixpkgs, wks }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs { 
      inherit system;
    };
  in {

    packages.x86_64-linux.demo = wks.functions.x86_64-linux.mkWks {
      name = "demoWks";
      packages = [ pkgs.hello pkgs.toilet ];
      inherit system;
      shellScript = "touch $out/hello.txt";
    };
  };
}
