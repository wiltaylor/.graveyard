inputs: final: prev:
let
  inherit (prev.vimUtils) buildVimPluginFrom2Nix;

  buildVitalityPlugin = name: buildVimPluginFrom2Nix {
    pname = name;
    version = "master";
    src = builtins.getAttr name inputs;
  };

  plugins = [
    "formatter-nvim"
    "fzf-lsp-nvim"
    "galaxyline-nvim"
    "nord-vim"
    "lspkind-nvim"
    "nvim-compe"
    "nvim-lspconfig"
    "nvim-tree-lua"
    "nvim-web-devicons"
    "scrollbar-nvim"
    "snippets-nvim"
    "vim-dadbod-ui"
    "vim-devicons"
    "vim-prisma"
    "vim-vsnip"
    "vimwiki"
  ];
in
{
  vitalityVimPlugins = builtins.listToAttrs
    (map
      (name:
        { inherit name; value = buildVitalityPlugin name; })
      plugins);

}
