@echo off
setlocal
set "EXE=%~dp0dist\01LowOS.exe"
if not exist "%EXE%" (
    echo Executavel nao encontrado: "%EXE%"
    pause
    exit /b 1
)
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Solicitando elevacao UAC unica para testar todas as melhorias...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b 0
)
echo Executando auto-teste completo (Aplicar -^> Verificar -^> Reverter)...
"%EXE%" --self-test
echo.
echo Relatorio salvo em C:\ProgramData\01LowOS\self_test_report.txt
pause
