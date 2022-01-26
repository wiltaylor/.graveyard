# What is this?
This is my personal nix pkgs overlay. I put all the extra packages I want to deploy on my systems here.

# How to use it
Simply add this flake to your config as an overlay.

Import flake:
```nix
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    nixpkgs-overlay.url = "github:wiltaylor/nixpkgs-overlay";
  };
```

Example from my nix config:
```
    allPkgs = lib.mkPkgs { 
      inherit nixpkgs; 
      cfg = { allowUnfree = true; };
      overlays = [
        nixpkgs-overlay.overlay
      ];
    };

```

# License
The files and scripts in this repository are licensed under the MIT License, which is a very 
permissive license allowing you to use, modify, copy, distribute, sell, give away, etc. the software. 
In other words, do what you want with it. The only requirement with the MIT License is that the license 
and copyright notice must be provided with the software.
