FROM archlinux:latest
RUN pacman -Syu --noconfirm
RUN pacman -Sy r base-devel make gcc pandoc-citeproc texlive-most python python-pip gcc-fortran python-graphviz --noconfirm
RUN Rscript -e 'install.packages(c("rmarkdown", "reticulate", "revealjs", "DiagrammeR", "devtools", "webshot", "bookdown"), repos = "https://mirror.las.iastate.edu/CRAN/"); webshot::install_phantomjs()'