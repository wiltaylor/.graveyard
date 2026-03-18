FROM warpdotdev/dev-base:latest

LABEL org.opencontainers.image.source="https://github.com/wiltaylor/wil-dev"
LABEL org.opencontainers.image.description="Personal development environment image with uv, playwright, and just"

# Install uv (Python package manager)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh

# Install playwright-cli
RUN npm install -g playwright-cli

# Install just (task runner)
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

# Install jq (JSON processor)
RUN apt-get update && apt-get install -y jq && rm -rf /var/lib/apt/lists/*

# Ensure PATH includes uv
ENV PATH="/root/.local/bin:$PATH"

WORKDIR /workspace
