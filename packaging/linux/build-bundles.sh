#!/usr/bin/env bash
# Gera os instaladores Linux das duas edições dentro do contêiner
# `multialt-linux-bundle` (docker/linux-bundle.Dockerfile). Sem assinatura de
# updater e sem publicar nada: é o mesmo que o linux-build.yml faz no CI.
#
#   /src  o repositório (montado só para leitura)
#   /out  onde caem os pacotes (nomes da release: MultiAlt_test-local_...)
#   /target  volume do cargo (para a segunda rodada não recompilar tudo)
#
# O node_modules e o target do Windows nunca entram: o repositório é copiado
# para /work sem eles e o bun instala os do Linux.
set -euo pipefail

VERSION_LABEL="${VERSION_LABEL:-test-local}"
EDITIONS="${EDITIONS:-standard full}"

mkdir -p /work /out
rsync -a --delete \
  --exclude node_modules --exclude dist --exclude target --exclude .git \
  --exclude 'src-tauri/target' --exclude graphify-out \
  /src/ /work/
cd /work
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/target}"

bun install --frozen-lockfile

build() {
  local edition="$1"; shift
  rm -rf "$CARGO_TARGET_DIR/release/bundle"
  bun tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}' "$@"
  bun .github/scripts/linux-release-assets.mjs stage \
    --edition "$edition" --version "$VERSION_LABEL" \
    --bundle "$CARGO_TARGET_DIR/release/bundle" --out "/out/$edition" --no-sign
}

for edition in $EDITIONS; do
  case "$edition" in
    standard)
      VITE_ENABLE_NEXUS=false VITE_ENABLE_WEBSERVER=false VITE_ENABLE_AVATAR_BATCH=true VITE_ENABLE_PYTHON_SCRIPTS=true \
        build standard -- --no-default-features --features standard ;;
    full)
      VITE_ENABLE_NEXUS=true VITE_ENABLE_WEBSERVER=true VITE_ENABLE_AVATAR_BATCH=true VITE_ENABLE_PYTHON_SCRIPTS=true \
        build full -- --features full ;;
    *) echo "unknown edition: $edition" >&2; exit 2 ;;
  esac
done

echo "Done. Packages in /out:"
ls -la /out/*
