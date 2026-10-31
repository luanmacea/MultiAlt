# Compila o backend para Linux nesta máquina (branch `linux`, 31/10/2026):
# a mesma lista de dependências do workflow `linux-check.yml`.
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
      libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev pkg-config build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src

# Uso (cada worktree com o seu volume de target, para não disputar a trava):
#   docker build -t multialt-linux-check -f docker/linux-check.Dockerfile docker
#   docker run --rm -v "<worktree>:/src" -v <volume>:/target -v mx-cargo:/usr/local/cargo/registry \
#     -e CARGO_TARGET_DIR=/target -e CARGO_TERM_COLOR=never multialt-linux-check \
#     bash -c "cd /src/src-tauri && cargo check -j 4 --no-default-features --features standard --message-format=short"
