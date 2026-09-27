# Builds VidCrop's Linux AppImage and .deb on Ubuntu 22.04, so they run on older
# distributions too (an AppImage only runs on systems at least as new as where it was built).
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential curl ca-certificates file git pkg-config xz-utils patchelf xdg-utils \
      libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://deb.nodesource.com/setup_24.x | bash - && apt-get install -y nodejs && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH=/root/.cargo/bin:/root/.local/bin:$PATH
WORKDIR /src
