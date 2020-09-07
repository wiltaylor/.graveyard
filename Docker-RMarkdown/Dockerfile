FROM archlinux:latest
RUN pacman -Syu --noconfirm
RUN pacman -Sy r base-devel make gcc pandoc-citeproc texlive-most python python-pip --noconfirm
RUN Rscript -e 'install.packages("rmarkdown", repos = "http://cran.us.r-project.org")' && Rscript -e 'install.packages("reticulate", repos = "http://cran.us.r-project.org")' && Rscript -e 'devtools::install_github("rstudio/rmarkdown")'

