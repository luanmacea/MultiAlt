# Clientes abertos fora do app (pelo site)

## Objetivo

Quando o dono abre um jogo pelo site do Roblox (e não pelo app), o cliente
precisa aparecer em Sessão → "Em jogo" como qualquer outro, para os cliques do
Modo AFK e a adoção do Auto Rejoin funcionarem nele. O mesmo mecanismo traz de
volta, depois de reiniciar o app, os clientes que já estavam abertos (antes
eles sumiam da Sessão, porque o `ProcessTracker` nasce vazio).

Automático quando dá; quando não dá, o cliente aparece como **"Cliente não
identificado"**, com **Mostrar janela** e **Identificar** (escolher a conta à
mão).

## Onde fica o código

| Arquivo | Papel |
|---|---|
| [platform/windows/external_clients.rs](../../src-tauri/src/platform/windows/external_clients.rs) | Parser do log, thread → PID (Toolhelp32), casamento log ↔ processo, decisão, `ExternalClientScanner` (cache por PID e por log), `scan_external_clients` |
| [platform/windows/tracker.rs](../../src-tauri/src/platform/windows/tracker.rs) | `track_adopted`: registra o PID na conta com `adopted: true` |
| [commands/external_clients.rs](../../src-tauri/src/commands/external_clients.rs) | Varredura em segundo plano (5 s, a primeira na hora em que o app abre), `get_unidentified_clients`, `identify_external_client`, `focus_client_window` |
| [commands/launch.rs](../../src-tauri/src/commands/launch.rs) | `get_running_instances` agora devolve `adopted` |
| [store.tsx](../../src/store.tsx) | `adoptedClients`, `unidentifiedClients` (mesmo polling de 2,5 s de `launchedByProgram`), `identifyExternalClient`, `focusClientWindow` |
| [SessionPanel.tsx](../../src/components/session/SessionPanel.tsx) | Marca "Aberto fora do app" e as linhas de cliente não identificado |

O macOS não tem o mecanismo: os comandos devolvem lista vazia / `false`.

## Como o cliente é reconhecido

O Roblox escreve um log por cliente em
`%LOCALAPPDATA%\Roblox\logs\<versão>_<YYYYMMDDTHHMMSSZ>_Player_<HEX>_last.log`
(os `_CrashHandler_` são ignorados). Formato de cada linha de dados:

```
2026-10-03T21:11:19.354Z,0.354978,3ed8,6,Warning [FLog::RobloxStarter] ...
```

1. **Qual log é de qual processo.** O 3º campo é o id (hex) da thread que
   escreveu a linha. As primeiras linhas saem da thread principal, que pertence
   ao processo enquanto ele roda: um retrato `TH32CS_SNAPTHREAD` dá o dono da
   thread (`th32OwnerProcessID`). Trava contra id de thread reaproveitado: a
   **origem do log** (horário da linha menos o 2º campo, o tempo decorrido)
   tem que cair entre 5 s antes e 60 s depois da criação do processo
   (`GetProcessTimes`). Medido em 03/10/2026: processo criado 21:11:16.709,
   origem do log 21:11:19.0.
2. **Reserva sem thread:** só pelo horário — origem do log entre 2 s antes e
   10 s depois da criação — e **só se o par for único dos dois lados**. Dois
   clientes abertos juntos não casam por aqui (viram "não identificado").
3. **De quem é.** A última linha `[FLog::GameJoinLoadTime] ... userid:<id>,`
   (teleporte repete; vale a última). O ticket do join
   (`joinScriptUrl ...ticket={"UserId"%3a<id>...`) também serve. O
   `rbxuid=` do cookie `RBXEventTrackerV2` **não** serve: o cookie é
   compartilhado entre contas e não prova quem está no cliente.
4. **Decisão** (`decide_external_client`):
   - conta salva e sem outro cliente vivo registrado → **adota**
     (`tracker.track_adopted`), igual a um launch do app;
   - sem log → `noLog`; log sem `userid:` ainda → `waitingForGame` (tenta de
     novo a cada varredura); conta fora da lista → `unknownAccount`; a conta já
     tem outro cliente vivo → `accountBusy`.
   Um PID registrado que já morreu não conta como ocupado: o cliente novo é
   adotado (é o caso do app reiniciado).

## Custo

- Sem `RobloxPlayerBeta` fora do tracker, a varredura é só a lista de
  processos (o mesmo snapshot que o resto do app já faz).
- Com candidato: lista a pasta de logs (só os modificados nas últimas 24 h e
  que não são mais velhos que o processo mais velho menos 1 min), lê só os
  primeiros 16 KB de cada log novo para o cabeçalho, e tira **um** retrato das
  threads. Se nada mudou desde a última tentativa (mesmos PIDs sem log, mesmos
  logs e tamanhos), não tira de novo.
- O log casado é lido **incrementalmente**: cada byte é lido uma vez (até
  8 MB por varredura), sempre até o último fim de linha.
- Medido na máquina do dono (build de debug, um cliente aberto): 59 ms na
  primeira varredura (lendo o log inteiro), 7 ms nas seguintes.

## Identificação manual

Linha "Cliente não identificado" em Sessão → "Em jogo", com o PID e o motivo:

- **Mostrar janela** — `focus_client_window(pid)`: só foca se o PID ainda for
  um `RobloxPlayerBeta` (PID reaproveitado nunca é focado).
- **Identificar** — escolhe a conta numa lista (nomes mascarados quando os
  nomes estão escondidos; contas já em jogo aparecem marcadas "(já em jogo)";
  se o log citou uma conta salva, ela já vem escolhida) →
  `identify_external_client(pid, userId)`. Se a conta já tinha outro cliente
  registrado, este passa a ser o dela e o outro vira "não identificado" na
  próxima varredura. Se o PID estava com outra conta (identificação errada),
  essa conta é desregistrada.

O cliente não identificado **não** entra na contagem "em execução" nem no lote
de "Fechar contas", e não tem botão de fechar: o app nunca fecha cliente que
não sabe de quem é.

## Regras / cuidados

- **Nada aqui fecha nem mexe no cliente.** Só lê a lista de processos, as
  threads e os logs (abertos só para leitura). Adotar é só registrar o PID.
- Depois de adotado, o cliente é igual a um lançado pelo app: Fechar/Focar da
  Sessão, cliques AFK, adoção do Auto Rejoin e as opções do Watcher que fecham
  cliente (memória, título, sem conexão — todas opt-in) passam a valer para
  ele. Fechar continua sendo ação explícita do usuário.
- **Limites:** o formato do log é do Roblox e pode mudar numa atualização
  (o parser falha fechado: sem cabeçalho ou sem `userid:` o cliente cai em
  "não identificado", com o caminho manual). O `userid:` só aparece depois
  que o cliente entra num jogo: um cliente parado no menu fica em
  `waitingForGame`. Um cliente com menos de 15 s de vida não é listado como
  não identificado (um launch do app ainda pode estar registrando o PID).
- Os testes (`external_client_log_tests`, `external_client_match_tests`,
  `external_client_scan_tests`, `external_client_command_tests`) usam um dublê
  do sistema (`ExternalClientOs`); a sonda da máquina de verdade
  (`external_client_real_probe`) é `#[ignore]` e só lê.
