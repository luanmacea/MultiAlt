#!/usr/bin/env bash
# Acha de qual commit do upstream este fork foi copiado.
#
# Não existe ancestral comum no git: o fork foi feito copiando arquivos, e o
# primeiro commit daqui é um commit raiz próprio. Mas hash de blob do git é do
# *conteúdo*, então é o mesmo nos dois repositórios. Este script conta quantos
# pares (caminho, blob) do primeiro commit deste fork aparecem em cada commit do
# upstream, e mostra os que mais batem.
#
# Só lê. Nenhum comando aqui escreve no repositório original.
#
# Uso:
#   find-fork-point.sh <upstream-dir> [<este-repo-dir>] [<branch-upstream>]
set -euo pipefail

UPSTREAM="${1:?caminho do repositório original}"
LOCAL="${2:-$(pwd)}"
BRANCH="${3:-v4}"

for dir in "$UPSTREAM" "$LOCAL"; do
  git -C "$dir" rev-parse --git-dir >/dev/null 2>&1 || {
    echo "não é um repositório git: $dir" >&2
    exit 1
  }
done

# Commit raiz deste fork: o estado copiado do upstream, antes de qualquer
# trabalho próprio. `--max-parents=0` pega a raiz mesmo com o histórico crescido.
BASE_COMMIT="$(git -C "$LOCAL" rev-list --max-parents=0 HEAD | tail -1)"
echo "commit raiz deste fork: $BASE_COMMIT  $(git -C "$LOCAL" log -1 --format='%cs %s' "$BASE_COMMIT")"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

git -C "$LOCAL" ls-tree -r "$BASE_COMMIT" | awk '{print $3" "$4}' | sort > "$TMP/base"
TOTAL="$(wc -l < "$TMP/base")"
echo "arquivos no commit raiz: $TOTAL"
echo

# Uma passada por commit. ~500 commits levam menos de um minuto.
for commit in $(git -C "$UPSTREAM" rev-list "$BRANCH"); do
  git -C "$UPSTREAM" ls-tree -r "$commit" | awk '{print $3" "$4}' | sort > "$TMP/up"
  printf '%s %s\n' "$(comm -12 "$TMP/base" "$TMP/up" | wc -l)" "$commit" >> "$TMP/scores"
done

echo "melhores candidatos (arquivos idênticos / total):"
sort -rn "$TMP/scores" | head -8 | while read -r n commit; do
  printf '  %s/%s  %s  %s\n' "$n" "$TOTAL" "$commit" \
    "$(git -C "$UPSTREAM" log -1 --format='%cs %s' "$commit")"
done

echo
echo "Empate no topo é esperado (merge commit e o commit anterior têm a mesma"
echo "árvore). Pegar o MAIS RECENTE dos empatados: é o último estado que o fork"
echo "já continha. Confirmar com:"
echo "  git -C '$LOCAL' rev-parse $BASE_COMMIT^{tree}"
echo "  git -C '$UPSTREAM' rev-parse <candidato>^{tree}"
