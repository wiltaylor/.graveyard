FROM jupyter/scipy-notebook:latest

ENV DEBIAN_FRONTEND noninteractive
ENV TERM linux

ENV LANGUAGE en_AU.UTF-8
ENV LANG en_AU.UTF-8
ENV LC_ALL en_AU.UTF-8
ENV LC_CTYPE en_AU.UTF-8
ENV LC_MESSAGES en_AU.UTF-8
ENV GOROOT=/usr/local/go 

ENV GONB_VERSION="v0.7.7"
ENV GOPATH="$HOME/go"
ENV NODE_VERSION=18.17.1
ENV NVM_DIR=/home/jovyan/.nvm
ENV PATH="$PATH:~/.cargo/bin:~/.dotnet/tools:/usr/local/go/bin:/home/jovyan/go/bin:/home/jovyan/.nvm/versions/node/v${NODE_VERSION}/bin/:/home/jovyan/.local/share/gem/ruby/3.0.0/bin"

USER root

RUN apt-get update \
	&& apt-get install -y build-essential curl libzmq3-dev pkg-config dotnet-sdk-7.0 wget apt-transport-https software-properties-common lua5.1 gnuplot graphviz libtool libffi-dev ruby ruby-dev make

RUN wget -q "https://packages.microsoft.com/config/ubuntu/20.04/packages-microsoft-prod.deb" \
	&& dpkg -i packages-microsoft-prod.deb \
	&& rm packages-microsoft-prod.deb \
	&& apt-get update \
	&& apt-get install -y powershell

RUN set -ex \
  && sed -i 's/^ en_AU.UTF-8 UTF-8$/en_AU.UTF-8 UTF-8/g' /etc/locale.gen \
  && locale-gen en_AU.UTF-8 \
  && update-locale LANG=en_AU.UTF-8 LC_ALL=en_AU.UTF-8

RUN wget https://go.dev/dl/go1.20.2.linux-amd64.tar.gz \
	&& tar -xvf go1.20.2.linux-amd64.tar.gz \
	&& mv go /usr/local/go \
	&& rm go1.20.2.linux-amd64.tar.gz

USER $NB_UID

RUN set -ex \
    && conda clean --all -f -y \
    && jupyter lab build -y \
    && jupyter lab clean -y \
    && rm -rf "/home/${NB_USER}/.cache/yarn" \
    && rm -rf "/home/${NB_USER}/.node-gyp" \
    && fix-permissions "${CONDA_DIR}" \
    && fix-permissions "/home/${NB_USER}"

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y

RUN cargo install --locked evcxr_jupyter \
	&& evcxr_jupyter --install

RUN mamba create -n cling \
	&& mamba install --quiet --yes -c conda-forge xeus-cling bash_kernel 

RUN dotnet tool install -g Microsoft.dotnet-interactive && dotnet interactive jupyter install

RUN mkdir /home/jovyan/.nvm \
	&& curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.4/install.sh | bash

RUN . "$NVM_DIR/nvm.sh" && nvm install ${NODE_VERSION} \
	&& nvm use v${NODE_VERSION} \
	&& nvm alias default v${NODE_VERSION}

RUN npm install -g tslab bash-language-server dockerfile-language-server-nodejs \
	typescript-language-server vscode-css-languageserver-bin vscode-html-languageserver-bin \
	vscode-json-languageserver-bin yaml-language-server \
	&& tslab install 

RUN gem install --user-install iruby \
	&& iruby register --force

RUN pip install ilua gnuplot_kernel dot_kernel -q \
	&& python -m gnuplot_kernel install --user \
	&& install-dot-kernel

RUN go install github.com/janpfeifer/gonb@latest \
	&& go install golang.org/x/tools/cmd/goimports@latest \
	&& go install golang.org/x/tools/gopls@latest \
	&& gonb --install

