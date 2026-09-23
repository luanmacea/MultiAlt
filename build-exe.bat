@echo off
setlocal enabledelayedexpansion
title Build - Roblox Account Manager 4
cd /d "%~dp0"

echo ===============================================
echo   Roblox Account Manager 4 - build do executavel
echo ===============================================
echo.

where bun >nul 2>&1
if errorlevel 1 (
    echo [ERRO] O "bun" nao foi encontrado no PATH.
    echo Instale em https://bun.sh e abra este script de novo.
    echo.
    pause
    exit /b 1
)

set "EXE=%~dp0src-tauri\target\release\roblox-account-manager.exe"

echo [1/2] Compilando... isso leva alguns minutos na primeira vez.
echo.
REM --no-bundle: gera so o .exe (sem instaladores). Passar pela CLI do Tauri e
REM obrigatorio: "cargo build" sozinho gera um exe que procura o servidor de
REM desenvolvimento em localhost:1420 e abre uma pagina de erro.
call bun run tauri build --no-bundle
if errorlevel 1 (
    echo.
    echo [ERRO] O build falhou. O erro esta acima.
    echo.
    pause
    exit /b 1
)

if not exist "%EXE%" (
    echo.
    echo [ERRO] O build terminou mas o executavel nao foi encontrado em:
    echo        %EXE%
    echo.
    pause
    exit /b 1
)

echo.
echo [2/2] Pronto. Abrindo a pasta com o executavel...
echo        %EXE%
echo.
echo Lembre-se: copie o .exe por cima do que voce usa hoje
echo (as contas e configuracoes ficam na pasta do executavel).
echo.
explorer /select,"%EXE%"

timeout /t 8 >nul
exit /b 0
