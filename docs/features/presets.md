# Presets de launch e agendamento

## Objetivo

Guardar "estas contas → este jogo/servidor" com um nome e abrir tudo com um clique, sem
selecionar conta por conta nem procurar o jogo de novo. Opcionalmente, abrir sozinho num
horário (dias da semana) e fechar num horário — fechando **só** os clientes que o próprio
preset abriu. É a ideia 13 de [ideias-de-outros-gerenciadores.md](../ideias-de-outros-gerenciadores.md)
(pacote Organização em [plano-ideias.md](../plano-ideias.md)).

## Onde fica o código

| Parte | Arquivo |
|---|---|
| Modelo, validação, gravação do arquivo, regras puras do horário (`due_actions`, `next_open_after`, `next_close_after`) | [data/launch_presets.rs](../../src-tauri/src/data/launch_presets.rs) |
| Comandos, o que cada execução abriu (`PRESET_RUNS`), fechar, agendador | [commands/launch_presets.rs](../../src-tauri/src/commands/launch_presets.rs) |
| Caminho do arquivo e `DATA_FILES` | [data/settings/paths.rs](../../src-tauri/src/data/settings/paths.rs) |
| Tela (lista + editor) | [components/presets/PresetsDialog.tsx](../../src/components/presets/PresetsDialog.tsx) |
| Regras puras da tela (destino a partir dos favoritos, rascunho, "Hoje 08:00") | [utils/presets.ts](../../src/utils/presets.ts) |
| Entradas | botão **Presets** da [Toolbar](../../src/components/layout/Toolbar.tsx) (lista de contas) e **Save as preset** no cabeçalho da [Choose Game](../../src/components/ChooseGameScreen.tsx) |
| Estado / launch pela tela | [store.tsx](../../src/store.tsx): `presetsDialog`, `openPresetsDialog`, `launchPreset`, ouvinte `launch-preset` |

## Fluxo

1. **Criar:** Toolbar › **Presets** › **New preset** (começa com as contas selecionadas), ou
   Choose Game › **Save as preset** (as contas da tela e o Place ID do campo, se houver).
   No editor: nome, contas (lista com caixas; nomes mascarados com "Names hidden"), jogo,
   grade e horário.
2. **Destino:** a lista "Game" traz cada favorito como "servidor público" e um item por VIP
   salvo nele (o mesmo `RAMGameLists.json` da Choose Game — [server-list.md](server-list.md)).
   O VIP vira `vip:<código>` (`vipJobFromLink`), a forma que a fila de várias contas entende;
   o preset guarda essa cópia, então apagar o favorito depois não estraga o preset. "Other
   game" aceita Place ID (ou link do jogo) e um Job ID opcional.
3. **Salvar:** `save_launch_preset` → `LaunchPresetStore::upsert` normaliza (`normalize_preset`)
   e grava. Id vazio = preset novo (`preset-<ms>`).
4. **Launch:** botão **Launch** da linha → aviso de conta online (`useJoinOnlineWarning`) →
   `store.launchPreset` → `launch_preset(id)` → `run_launch_preset`:
   1. só as contas do preset que ainda existem no app;
   2. retrato do tracker (conta → PID) **antes**;
   3. `launch_multiple(...)` — a **mesma** fila de sempre, chamada pela porta pública dela, sem
      mudança: reserva de sequência ("um launch por vez"), piso anti-captcha, guarda de versão,
      isolamento, Multi Roblox ([multi-launch.md](multi-launch.md));
   4. retrato **depois**; a conta do preset que tem PID novo (e não é cliente do site) entra em
      `PRESET_RUNS[preset]` (`clients_opened_by_run`, `merge_preset_run`);
   5. com **Arrange the windows in a grid when done**, chama o mesmo `arrange_windows_grid` do
      botão da aba Windows, com `GridMonitors`/`GridGap`.
5. **Fechar:** **Close them** (aparece quando o preset tem cliente aberto) ou o horário de
   fechar → `close_preset_run`: para cada cliente da execução, **confere de novo** que o tracker
   ainda tem aquele PID para aquela conta e que não é cliente adotado do site, e só então
   `kill_for_user`.
6. **Resultado:** linha no Console (`step: "preset"`, sem conta) e evento `launch-preset`
   (`presetId`, `name`, `action`, `scheduled`, `ok`, `count`, `error`). O que veio do horário
   vira toast (ninguém clicou).

## Agendamento

- **Só com o app aberto.** Uma task do tokio (`start_preset_scheduler`, ligada no `setup` do
  [lib.rs](../../src-tauri/src/lib.rs)) olha o relógio a cada 15 s. Nada de tarefa agendada do
  Windows, entrada de inicialização ou serviço — travado por
  `the_scheduler_never_uses_the_windows_task_scheduler`.
- **Sem recuperação.** Cada passada avalia só o intervalo desde a passada anterior. Se o app
  estava fechado na hora, aquela vez não acontece ao abrir. Intervalo maior que 3 min entre
  duas passadas (PC dormindo, relógio pulando) também é descartado
  (`preset_scheduler_window`). O agendador começa a contar no instante em que o app abre.
- **Abrir** usa os dias marcados (0 = segunda … 6 = domingo). **Fechar** vale todo dia naquele
  horário e só faz algo se o preset tiver cliente aberto — senão um preset que abre segunda às
  22:00 e fecha às 02:00 nunca fecharia (já é terça).
- **Uma ação por vez:** as ações vencidas entram numa fila própria; dois presets às 08:00
  abrem um depois do outro, e a passada do relógio nunca espera um launch.
- **Launch em andamento na hora:** a abertura agendada tenta de novo a cada 15 s por até
  10 min (recusa `launch-already-active`); depois disso desiste e avisa.
- A próxima abertura e o próximo fechamento aparecem na linha do preset ("Opens Mon 08:00 ·
  Closes Today 18:00"), calculados no backend (`next_open_after`/`next_close_after`, hora local).

## Regras de negócio

- **Fechar nunca passa do que o preset abriu.** Cliente do site (adotado), de outra conta, o
  que já estava aberto antes da execução, o que foi reaberto depois por outro caminho (Auto
  Rejoin, reconexão, o próprio usuário) e PID que o Windows reaproveitou — nenhum é tocado.
  Nada aqui chama `cmd_kill_all_roblox` (teste `closing a preset's windows ... never closes
  everything`). Ver as regras críticas do `CLAUDE.md`.
- **O registro de execuções vive em memória.** Ao reiniciar o app, ele começa vazio: o horário
  de fechar não fecha clientes abertos antes do reinício (não dá para afirmar de quem eram).
- Apagar o preset esquece a execução dele (não fecha nada).
- Conta que saiu do app é ignorada no launch e sinalizada na linha ("N account(s) of this preset
  are no longer in the app").
- Sem refresh de sessão: o launch é o da fila, que usa o ticket normal; nada aqui chama
  `run_with_session_retry`/`refresh_account_session` por conta própria.
- Limites: nome até 60 caracteres, até 200 contas por preset, até 100 presets.

## Arquivo

`RAMLaunchPresets.json`, na pasta de dados:

```json
{ "presets": [ { "id": "preset-1760000000000", "name": "Morning farm",
  "userIds": [1, 2], "placeId": 6516141723, "jobId": "", "gameName": "Blox Fruits",
  "vipName": null, "arrangeGrid": true,
  "schedule": { "openEnabled": true, "openAt": "08:00", "days": [0,1,2,3,4],
                "closeEnabled": true, "closeAt": "18:00" },
  "createdAt": 1760000000000 } ] }
```

- Gravação atômica (`.tmp` + troca) e a versão anterior em `RAMLaunchPresets.json.bak`.
- Arquivo ilegível **não é sobrescrito**: a gravação falha com a mensagem do erro.
- O store relê o disco a cada leitura: restaurar um backup vale na hora, sem reiniciar.
- Está em `DATA_FILES`: entra no backup, na restauração e na migração de pasta
  ([backups.md](backups.md)). Teste: `restoring_a_backup_brings_the_launch_presets_back_without_a_restart`.

## Harness

`bun run dev:ui` e abra `?scenario=presets&accounts=6`: três presets (um com horário e dois
clientes abertos, um com VIP dos favoritos, um com conta que saiu do app). Toolbar › Presets.

## Testes

`bun run t presets` — Rust: `launch_preset_store_tests`, `launch_preset_validation_tests`,
`launch_preset_schedule_tests`, `launch_preset_run_tests`; front:
[PresetsDialog.test.tsx](../../src/components/presets/PresetsDialog.test.tsx),
[presets.test.ts](../../src/utils/presets.test.ts).

## Armadilhas / cuidados

- **Não verificado com cliente real** (precisa do teste do dono): o agendamento disparando com o
  app minimizado na bandeja e o fechamento pegando só os clientes da execução.
- O launch agendado não pergunta "conta online?" (não há ninguém para responder); o botão da
  tela pergunta.
- A grade depois do launch é a mesma do botão manual: organiza **todas** as janelas do Roblox
  abertas (as contas com janela própria ficam de fora), não só as do preset.
- macOS: o launch funciona pelo `launch_multiple` de lá; a grade não existe no Mac.
