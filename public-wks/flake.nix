{
  description = "A list of my public workspaces";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    wks.url = "github:wiltaylor/nixwks";
  };

  outputs = { self, nixpkgs, wks}: let
    lib = import ./lib;
    allPkgs = lib.mkPkgs { inherit nixpkgs; cfg = { allowUnfree = true;};};

  in {
    packages = lib.withDefaultSystems (sys: let 
      pkgs = allPkgs."${sys}";
      mkWks = wks.functions."${sys}".mkWks;

    in {
      dev = mkWks {
        name = "dev";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [

          #Rust stuff
          rustup

          # Go stuff
          go
          delve

          # C and CPP
          gnumake
          gcc_latest
          clang_13
          cmake
          emscripten

          # SDL Libs
          SDL2
          SDL2_gfx
          SDL2_mixer
          SDL2_net
          SDL2_ttf
          SDL2_image

          # dotnet
          dotnet-runtime
          dotnet-netcore
          dotnet-aspnetcore
          jetbrains.rider

          # Python
          python3Full

          # Ruby
          ruby

          # javascript
          nodejs

        ];

        startHook = ''
          ln -sf "$REALHOME/repo" "$HOME/repo"
        '';
      };

      youtube = mkWks {
        name = "youtube";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [
          mpv
          youtube-dl
          pipe-viewer
          gtk-pipe-viewer
        ];

        guiScript = ''
          OPT=$(echo -e "Pipe Viewer" | rofi -dmenu)
          case $OPT in
          "Pipe Viewer")
            exec gtk-pipe-viewer
          ;;
           *)
          ;;
          esac
        '';
      };


      video = mkWks {
        name = "video";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [
          blender
          mpv
          gimp
          youtube-dl
          ytfzf
          obs-studio
          kdenlive
          ardour
          tenacity
        ];

        guiScript = ''
          OPT=$(echo -e "Blender\nGimp\nOBS\nKdenlive\nArdour\nTenacity (Audacity Fork)" | rofi -dmenu)
          case $OPT in
          "Blender")
            exec blender
          ;;
          "Gimp")
            exec gimp
          ;;
          "OBS")
            exec obs
          ;;
          "Kdenlive")
            exec kdenlive
          ;;
          "Ardour")
            exec ardour6
          ;;
          "Tenacity (Audacity Fork)")
            exec tenacity
          ;;
          *)
          ;;
          esac
        '';

      };

      browsers = mkWks {
        name = "browsers";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [
          firefox
          google-chrome
          vivaldi
          brave
          chromium
          nyxt
        ];

        startHook = ''
          export MOZ_USE_XINPUT2=1 
          ln -sf "$REALHOME/Downloads" "$HOME/Downloads"
        '';

        guiScript = ''
          OPT=$(echo -e "Firefox\nGoogle Chrome\nChromium\nVlivaldi\nBrave\nNyxt" | rofi -dmenu)
          case $OPT in
          "Firefox")
            exec firefox
          ;;
          "Google Chrome")
            exec google-chrome-stable
          ;;
          "Chromium")
            exec chromium 
          ;;
          "Vivaldi")
            exec vivalid
          ;;
          "Brave")
            exec brave
          ;;
          "Nyxt")
            exec nyxt
          ;;
          *)
          ;;
          esac
        '';
      };

      games = mkWks {
        name = "games";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [ 
          steam 
          steam-run 
          lutris

          # full wine
          wineWowPackages.stagingFull
          winetricks

          # Linux games
          xonotic 
          minecraft 
          quakespasm 
          superTuxKart 
          openmw
        ];

        startHook = ''
          ln -sf "$REALHOME/.pki" "$HOME/.pki"
          ln -sf "$REALHOME/.config/cef_user_data" "$HOME/.config/cef_user_data"
          ln -sf "$REALHOME/.config/pulse" "$HOME/.config/pulse"
        '';

        guiScript = ''
          OPT=$(echo -e "Steam\nLutris\nXonotic\nMinecraft\nQuake\nSuper Tux Kart\nMorrowind" | rofi -dmenu)
          case $OPT in
          "Steam")
            exec steam
          ;;
          "Lutris")
            exec lutris
          ;;
          "Xonotic")
            exec xonotic
          ;;
          "Minecraft")
            exec minecraft-launcher
          ;;
          "Quake")
            exec quake
          ;;
          "Super Tux Kart")
            exec supertuxkart
          ;;
          "Morrowind")
            exec openmw-launcher
          ;;
          *)
          ;;
          esac
        '';
      };

      office = mkWks {
        name = "office";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [ wpsoffice foxitreader onlyoffice-bin libreoffice ];
        guiScript = ''
          OPT=$(echo -e "WPS Office\nOnly Office\nFoxitReader" | rofi -dmenu)
          case $OPT in
          "WPS Office")
            exec wps
          ;;
          "Only Office")
            exec onlyoffice-desktopeditors
          ;;
          "FoxitReader")
            exec FoxitReader
          ;;
          *)
          ;;
          esac
        '';
      };

      orgSys = mkWks {
        name = "orgSys";
        system = sys;
        startHook = ''
          ln -sf "$REALHOME/vaults" "$HOME/vaults"
        '';
        homeIsolation = true;
        packages = with pkgs; [ 
          obsidian 
          foxitreader 
          xmind 
          zotero 
          rofi

          # Hack that makes obsidian links work in the workspace.
          (writeScriptBin "www-browser" ''
            obsidian $@
          '')


        ];
        guiScript = ''
          OPT=$(echo -e "Obsidian\nZotero\nXMind\nFoxitReader" | rofi -dmenu)
          case $OPT in
          "Obsidian")
            exec obsidian
          ;;
          "Zotero")
            exec zotero
          ;;
          "FoxitReader")
            exec FoxitReader
          ;;
          "XMind")
            exec XMind
          ;;
          *)
          ;;
          esac
        '';
      };

      torrent = mkWks {
        name = "torrent";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [
          (writeShellApplication {
            name = "torrent";
            text = ''
              # shellcheck disable=SC1091
              source "$HOME/torrent.env"

              Usage() {
                echo "Torrent Util usage:"
                echo "torrent {command}"
                echo ""
                echo "Commands:"
                echo "up - Start the torrent and vpn connection"
                echo "down - Stop all torrent containers"
                echo "ls - List active torrents"
                echo "add {url} - Add a torrent to download"
                echo "start {id} - Start torrent"
                echo "stop {id} - Stop torrent"
                echo "rm {id} - Remove torrent"
                echo "browser - Runs tor browsers"
              }

              browser() {
                docker run -t -i --rm --name tor-browser \
                -v /tmp/.X11-unix:/tmp/.X11-unix \
                -v /dev/shm:/dev/shm \
                -e DISPLAY="unix$DISPLAY" \
                 --net=container:protonvpn \
                inglebard/tor-browser --log /dev/stdout --class=tor-docker


              }

              up() {
                DOWNLOADPATH="$HOME/downloads"
                docker run \
                --rm \
                --detach \
                --name=protonvpn \
                --device=/dev/net/tun \
                --cap-add=NET_ADMIN \
                --env PROTONVPN_USERNAME="$PROTONVPN_USERNAME" \
                --env PROTONVPN_PASSWORD="$PROTONVPN_PASSWORD" \
                --env PROTONVPN_TIER=3 \
                --env PROTONVPN_SERVER=P2P \
                ghcr.io/tprasadtp/protonvpn:latest

                docker run \
                    -d \
                    --name transmission \
                    -v "$DOWNLOADPATH:/data" \
                    -e USERNAME="$TRANSMISSION_USER" \
                    -e PASSWORD="$TRANSMISSION_PASSWORD" \
                    --net=container:protonvpn \
                    wiltaylor/transmission
              }

              down() {
                docker rm -f transmission protonvpn
              }

              lstorrent() {
                docker exec transmission transmission-remote -n "$TRANSMISSION_USER:$TRANSMISSION_PASSWORD" -l
              }

              addtorrent() {

                docker exec transmission transmission-remote -n "$TRANSMISSION_USER:$TRANSMISSION_PASSWORD" -a "$1"
              }

              starttorrent() {

                docker exec transmission transmission-remote -n "$TRANSMISSION_USER:$TRANSMISSION_PASSWORD" -t "$1" -s

              }

              stoptorrent() {

                docker exec transmission transmission-remote -n "$TRANSMISSION_USER:$TRANSMISSION_PASSWORD" -t "$1" -S
              }

              rmtorrent() {

                docker exec transmission transmission-remote -n "$TRANSMISSION_USER:$TRANSMISSION_PASSWORD" -t "$1" -r
              }

              case "$1" in
              "up")
                up
              ;;
              "down")
                down
              ;;
              "ls")
                lstorrent
              ;;
              "add")
                addtorrent "$2"
              ;;
              "start")
                starttorrent "$2"
              ;;
              "stop")
                stoptorrent "$2"
              ;;
              "rm")
                rmtorrent "$2"
              ;;
              "browser")
                browser
              ;;
              esac
          '';
          })
        ];
      };
    });
  };
}
