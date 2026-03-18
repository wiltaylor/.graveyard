{ pkgs }:

with pkgs;
{

  bashls = {
    lspConfig = {
      cmd = [ "${pkgs.nodePackages.bash-language-server}/bin/bash-language-server" "start" ];
    };
  };

  dockerls = {
    lspConfig = {
      cmd = [ "${nodePackages.dockerfile-language-server-nodejs}/bin/docker-langserver" "--stdio" ];
    };
  };


  gopls = {
    lspConfig = {
      cmd = [ "${gopls}/bin/gopls" ];
    };
  };

  kotlin_language_server = {
    lspConfig = {
      cmd = [ "${nur.repos.zachcoyle.kotlin-language-server}/bin/kotlin-language-server" ];
    };
  };

  pyright = {
    lspConfig = {
      cmd = [ "${nodePackages.pyright}/bin/pyright-langserver" "--stdio" ];
    };
  };

  rnix = {
    lspConfig = {
      cmd = [ "${rnix-lsp}/bin/rnix-lsp" ];
    };
  };

  solargraph = {
    lspConfig = {
      cmd = [ "${solargraph}/bin/solargraph" "stdio" ];
    };
  };

  rust_analyzer = {
    lspConfig = {
      cmd = [ "${rust-analyzer}/bin/rust-analyzer" ];
    };
  };

  terraformls = {
    lspConfig = {
      cmd = [ "${terraform-ls}/bin/terraform-ls" "serve" ];
    };
  };

  tsserver = {
    lspConfig = {
      cmd = [ "${nodePackages.typescript-language-server}/bin/typescript-language-server" "--stdio" ];
      filetypes = [ "javascript" "javascriptreact" "javascript.jsx" "typescript" "typescriptreact" "typescript.tsx" "json" "jsonc" ];
    };
  };

  vimls = {
    lspConfig = {
      cmd = [ "${nodePackages.vim-language-server}/bin/vim-language-server" "--stdio" ];
    };
  };

  yamlls = {
    lspConfig = {
      cmd = [ "${nodePackages.yaml-language-server}/bin/yaml-language-server" "--stdio" ];
    };
  };
}
