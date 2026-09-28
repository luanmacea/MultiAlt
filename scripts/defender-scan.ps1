# Escaneia UM arquivo com o Windows Defender (MpCmdRun), sob demanda.
#
#   powershell -File scripts/defender-scan.ps1 <caminho-do-exe>
#
# Espelha o que o usuario ve em "Protecao contra virus e ameacas": usa o mesmo
# motor e as mesmas definicoes. So leitura — nao instala nem altera nada.
#
# Saida: "LIMPO" quando o Defender nao acha nada, ou a(s) ameaca(s) achada(s).
# Codigo de saida 0 = limpo, 2 = ameaca encontrada, 1 = erro.
param([Parameter(Mandatory = $true)][string]$Arquivo)

if (-not (Test-Path -LiteralPath $Arquivo)) {
    Write-Output "arquivo nao encontrado: $Arquivo"
    exit 1
}
$full = (Resolve-Path -LiteralPath $Arquivo).Path

# Prefere a plataforma mais nova (definicoes atualizadas); cai para o caminho fixo.
$mp = Get-ChildItem "$env:ProgramData\Microsoft\Windows Defender\Platform\*\MpCmdRun.exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]($_.Directory.Name -replace '-.*$', '') } -ErrorAction SilentlyContinue |
    Select-Object -Last 1 -ExpandProperty FullName
if (-not $mp) { $mp = "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" }
if (-not (Test-Path $mp)) { Write-Output "MpCmdRun.exe nao encontrado"; exit 1 }

Write-Output "arquivo : $full"
Write-Output "tamanho : {0:N1} MB" -f ((Get-Item $full).Length / 1MB)
Write-Output "motor   : $mp"
Write-Output ""

# -ScanType 3 = escanear um arquivo/pasta especifico. -DisableRemediation nao
# apaga nada: so relata. Sem isso, o Defender poderia colocar em quarentena.
$saida = & $mp -Scan -ScanType 3 -File $full -DisableRemediation 2>&1 | Out-String
$code = $LASTEXITCODE
Write-Output $saida.Trim()
Write-Output ""

if ($saida -match "was not found|no threats|found no threats" -or $code -eq 0) {
    Write-Output "veredito: LIMPO (Windows Defender nao marcou o arquivo)"
    exit 0
}
if ($saida -match "found|Threat|Trojan|Wacatac") {
    Write-Output "veredito: AMEACA — o Windows Defender marcou o arquivo"
    exit 2
}
Write-Output "veredito: inconclusivo (codigo $code) — leia a saida acima"
exit 1
