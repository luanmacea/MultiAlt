# Console

A página **Console** ([ConsolePage.tsx](../../src/components/pages/ConsolePage.tsx), [LaunchConsole.tsx](../../src/components/console/LaunchConsole.tsx)) mostra o histórico do que o app fez: launch, Auto Rejoin, Watcher, reconexão, memória, gravações, presets e erros. Cada linha tem hora, a origem (`step`, em colunas de largura fixa) e o nome da conta (ou "—" para linha de sessão, sem conta). As listas da sessão (Entrando, Em jogo) moram só na página Session; no topo do Console fica a grade de janelas (só Windows).

## O evento `launch-log`

O backend escreve uma linha com `emit_launch_log` (uma conta) ou `emit_session_log` (`userId` nulo) — [launch_shared.rs](../../src-tauri/src/commands/launch_shared.rs). O payload é montado por `launch_log_payload` ([log_msg.rs](../../src-tauri/src/log_msg.rs)):

```json
{ "userId": 42, "level": "success", "step": "pid",
  "message": "Client started (PID 4242)",
  "key": "Client started (PID {{pid}})", "params": { "pid": 4242 } }
```

- `message`: a frase **em inglês** já montada. É o que valem relatório de problema, ouvinte antigo e linha sem `key`.
- `key` + `params`: a frase em inglês com `{{parametros}}` e os valores. A `key` é também a chave do catálogo (`src/locales/<idioma>/common.json`, inglês como fonte). O Console faz `t(key, params)` **na hora de desenhar** ([utils/launchLog.ts](../../src/utils/launchLog.ts)), então trocar o idioma traduz as linhas que já estão na tela. Frase sem tradução cai no inglês da própria chave.
- Um parâmetro pode ser outra frase (`{ "key": "public server", "params": {} }`) ou uma lista delas: a tela traduz cada uma antes de entrar na frase de fora (ex.: "Starting launch — place … (servidor público)").
- Linha crua (`LogMsg::raw`, vinda de `String`): sem `key`, mostra o `message` como veio. Só para texto que já vem pronto de fora.
- Níveis: `info`, `success`, `warn`, `error` (só cor). O `step` é um código de máquina em inglês (`start`, `auth`, `pid`, `rejoin`, `watcher`...) e aparece como está, entre colchetes.

## Regra: o Console nunca recebe português nem frase montada à mão

1. **Nenhuma linha do Console é um literal em português** (nem em outro idioma): a frase é inglês, com `{{parametros}}`, escrita com `log_msg!("... {{valor}} ...", valor = x)` ou como literal solto sem parâmetro (`emit_launch_log(app, uid, "info", "auth", "Authentication ticket obtained")`).
2. **Texto de fora entra como parâmetro**, nunca dentro da frase: o erro do Roblox/Windows, o nome do preset, a mensagem de expulsão. A frase em volta é traduzida; o texto de fora fica como veio.
3. **Nada de `format!`/`String::from`/`.to_string()` como frase** de `emit_*`: isso vira linha crua, sem tradução. Função que devolve uma frase devolve `LogMsg` (`drop_console_line`, `memory_console_line`, `reconnect_console_line`, `moderation_block_log`, `launch_target_description`...); quando a mesma frase também vai para um erro de tela, o `String` sai de `.render()` (inglês).
4. **Frase nova = chave nos quatro catálogos** (en, pt, es, de), com os mesmos `{{placeholders}}`. `bun scripts/i18n/extract-keys.ts` só lê o frontend: as frases do Rust entram à mão.
5. Evite o parâmetro `count` (o i18next trata como plural) e cuidado com o tom: o teste de tom do `locales.test.ts` exige que a tradução de uma frase com "failed"/"started"/"saved" continue dizendo isso ("falha", "iniciado", "salvo"...).

### A trava

`console_language_tests` (dentro de [log_msg.rs](../../src-tauri/src/log_msg.rs)) lê o fonte de produção do backend, acha cada `log_msg!` e cada literal solto de `emit_launch_log`/`emit_session_log` e reprova quando: a frase tem acento ou palavra de português; algum `{{parametro}}` fica sem valor (ou o contrário); a frase falta em algum dos quatro catálogos; ou alguma linha é montada à mão. `LaunchConsole.test.tsx` cobre a tela: a mesma linha em en/pt/es/de, troca de idioma com linhas já na tela, frase aninhada e texto de fora intocado. Rodar: `bun run t botting`.

## Exceção conhecida

O aviso da **versão antiga do Roblox** (`resolve_launch_install` em launch_shared.rs e o aviso de versão que sumiu, em `resolve_roblox_install_path_with_notice`) ainda sai como linha crua em inglês: é montado por outro trabalho em andamento e fica listado em `RAW_ENGLISH_ALLOWED` no teste. Converter para `log_msg!` e tirar da lista quando esse bloco assentar.

O `step` não é traduzido (é um código, como `[watcher]`), e as mensagens de erro de **fila de launch** (painel Session) são outro caminho: ver [launch.md](launch.md).
