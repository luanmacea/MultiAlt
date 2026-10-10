# Plano das ideias de outros gerenciadores

Ordem de trabalho das ideias de [ideias-de-outros-gerenciadores.md](ideias-de-outros-gerenciadores.md)
(números iguais aos de lá), combinada com o dono em 09/10/2026.

## Regras do plano

- **Poucas atualizações, cada uma com um tema.** Já temos usuários: cada versão
  junta um pacote coerente, em vez de uma versão por funcionalidade.
- **Uma branch por pacote** (`feature/<tema>`), com merge na `develop` só depois de:
  `bun run check`, build da **edição padrão** igual à da release e
  `bun run scan` (Defender + VirusTotal) **limpo**. Funcionalidade que fizer o
  `.exe` ser marcado fica fora, ou atrás de um interruptor, até a decisão do dono.
- **Fácil de entender:** opção nova vem desligada ou com padrão seguro, com uma
  frase dizendo o que faz; nada de tela nova onde um interruptor resolve.

## Ritmo das versões

**Uma versão a cada ~2 dias**, no máximo (pedido do dono, 09/10/2026). Antes
de subir a próxima:

1. Olhar o que chegou desde a última: issues e "Enviar feedback", Discussions,
   contagem de downloads/atualizações da versão atual.
2. **Problema novo na versão atual vem primeiro**: vira hotfix ou entra no
   próximo pacote, e o pacote pode encolher para não atrasar a correção.
3. Só então: PR da `develop` para a `main` com o "What's new" do pacote.

| Versão | Pacote | Pronto na develop | Publicada | Observações |
|---|---|---|---|---|
| 1.1.0 | telas pequenas, feedback, tamanho da interface | — | 08/10/2026 | |
| 1.2.0 | **Conta** (8, 10, 26, 7, 9) + ícones na legenda de status | 10/10/2026 | 10/10/2026 | "minor update"; `.exe` e MSI limpos no Defender e VirusTotal |
| próxima | **Quedas** (1, 19, 4) | branch `feature/quedas` | — | não antes de 12/10/2026; falta teste do dono com cliente real |

## Pacotes

| Versão | Tema | Ideias | Por que nessa ordem |
|---|---|---|---|
| 1.2 | **Quedas** | 1, 19, 4 | É o maior buraco do app (6 de 48 concorrentes já têm). A 19 precisa vir junto da 1: renomear a janela quebraria a detecção atual pelo título. |
| 1.3 | **Reconexão** | 2, 14, 23 | Usa a detecção do 1.2. A fila que espera o jogo carregar (14) sai de graça do mesmo leitor de log; manter o PC acordado (23) é o que o AFK longo precisa. |
| 1.4 | **Conta** | 8, 10, 26, 7, 9 | Leituras simples e baratas, sem código nativo novo. |
| 1.5 | **Desempenho** | 18, 20, 22 | Otimização que segue o foco, volume ao vivo, grade menor. Mexe com APIs nativas: precisa de cuidado com antivírus. |
| 1.6 | **Organização** | 13, 6 | Presets/agendamento e histórico de sessões. |
| depois | — | 24, 25, 27, 28, 21, 30 | Bons, sem urgência. A 30 (Mac) depende de alguém testar num Mac. |
| teste | **Multi Roblox** | 3 | Ver abaixo: pode até **reduzir** o risco de antivírus. Só com teste real. |

## Respostas às dúvidas do dono

- **3 — reservar o nome `ROBLOX_singletonEvent`.** Hoje, para abrir mais de um
  Roblox, o app **entra nos processos do Roblox e fecha um "sinal" lá dentro**
  (`close_roblox_singleton_handles`, em `singleton.rs`). Quando um cliente
  teleporta de jogo, ele recria esse sinal e um dos clientes pode fechar. A
  ideia é o app criar antes um objeto com o mesmo nome: o Roblox não consegue
  recriar o sinal, e **ninguém fecha**. A parte de "fazer as outras fecharem"
  que você lembra é de **outro** projeto (o evanovar mata os clientes antes) —
  isso não entra. Bônus: se funcionar, dá para parar de mexer nos processos do
  Roblox, que é justamente o tipo de código que antivírus estranham. Precisa de
  teste com você teleportando entre places com 2+ contas abertas.
- **7 — gravar o cookie novo.** Às vezes o Roblox troca o cookie da conta numa
  resposta. Hoje só guardamos o novo em dois casos (sair das outras sessões e
  trocar senha). Se ele trocar em outra hora, a conta salva fica com o cookie
  velho e aparece "inválida" sem motivo. É barato e só evita problema; entra no
  pacote Conta, sem urgência, já que ninguém relatou isso.
- **11 — tracker do aparelho.** Um autor mediu o cliente travando 5–8 s quando
  cada conta usa um identificador de navegador próprio. Não sabemos se acontece
  aqui: **só medir**, sem mudar nada, quando houver tempo.
- **14 — fila que espera o jogo carregar (validação: vale).** Hoje a fila espera
  um tempo fixo entre contas. Com o leitor de log do pacote Quedas ela passa para
  a próxima assim que a anterior entra no jogo (com teto). Abrir 10 contas fica
  mais rápido e com menos chance de captcha. Entra no 1.3.
- **15 — conferir a assinatura do `RobloxPlayerBeta.exe` (validação: não agora).**
  As builds já vêm do CDN oficial do Roblox, baixadas pelo próprio app. O ganho é
  pequeno e exige uma API nativa nova no binário (`WinVerifyTrust`), que é risco
  de antivírus. Fica de fora.
- **16 — diagnóstico "o launch não faz nada".** Uma tela de checagem (Roblox
  instalado, pasta com permissão, internet, processos do Roblox presos sem janela)
  para quando alguém relata "clico e nada acontece". Útil para suporte quando
  chegarem relatos pelo "Enviar feedback"; sem urgência.
- **17 — configurações em três camadas.** Configuração geral → por jogo → por
  conta (ex.: FPS diferente num jogo). Poderoso, mas complica a tela. Não vale
  agora.
- **29 — catálogo de versões mais robusto.** Baixar builds do Roblox por mais de
  um servidor do CDN (hoje só um) e mostrar as FastFlags que o Roblox recusou.
  Só importa para quem usa versões customizadas; baixa prioridade.

## Ideias não marcadas que valem citar

- **5 — aviso no Discord** combina com 1 e 2 (avisar quando uma conta cai), mas
  manda dados para fora do PC: só se o dono quiser, desligado por padrão.
- **12 — adicionar conta por Quick Login** (aprova no celular, sem digitar senha
  no app): bom para quem tem medo de colar cookie. Candidata a um pacote futuro.
