# Gera os instaladores Linux (AppImage, .deb, .rpm) nesta máquina (branch
# `linux`, 31/10/2026). Mesmo sistema do CI (ubuntu 22.04: o AppImage herda a
# glibc de quem o compila, então quanto mais velha, em mais distros ele abre).
# A lista de pacotes é a do job `linux` do release-v4.yml e do linux-build.yml.
#
# Uso (a partir da raiz do repositório; ver docs/development.md › Linux):
#   docker build -t multialt-linux-bundle -f docker/linux-bundle.Dockerfile docker
#   docker run --rm -v "<worktree>:/src:ro" -v "<pasta de saída>:/out" \
#     -v mx-bundle-target:/target -v mx-cargo:/root/.cargo/registry \
#     multialt-linux-bundle bash /src/packaging/linux/build-bundles.sh
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup \
    PATH=/root/.cargo/bin:/root/.bun/bin:$PATH \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    CARGO_TERM_COLOR=never
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl unzip git rsync gnupg \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
      libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev libxdo-dev \
      pkg-config build-essential patchelf file xdg-utils \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal \
    && curl -fsSL https://bun.sh/install | bash
WORKDIR /work
