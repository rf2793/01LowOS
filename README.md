# 01LowOS — Otimizador Modular para Windows 11 em Rust

Aplicativo desktop nativo em Rust (`egui`/`eframe`), gratuito e open-source (GPL-3.0), para gerenciar, aplicar, verificar e reverter **individualmente** **87 otimizações** de latência, GPU, rede, energia, privacidade e serviços no Windows 11 (`24H2` / `25H2` / `26H1`), com snapshot e backup atômico por ajuste.

- **Download do executável (Releases):** https://github.com/rf2793/01LowOS/releases
- **Reportar problemas:** https://github.com/rf2793/01LowOS/issues
- **Apoiar o projeto:** https://github.com/sponsors/rf2793 | **PIX:** `be040dc9-6c7c-48b6-af4a-6a0444a5eb45`

## Destaques

1. **Melhorias Individuais (87 ajustes granulares)**:
   - Organizadas por categoria (`GPU & Latência`, `Sistema & Kernel`, `Energia`, `Rede`, `Usuário (HKCU)`, `Privacidade & Bloat`, `Serviços Windows`, `Terceiros`, `Tarefas Agendadas`, `ETW Autologgers`).
   - Cada melhoria grava snapshot do estado original em `C:\ProgramData\01LowOS\Backups\tweaks\<id>.json` (ou `%LOCALAPPDATA%\01LowOS\Backups\tweaks\` sem elevação) antes de alterar o sistema.
   - Barra de progresso ao vivo, **Preset Jogos Seguro (1-clique)**, **Selecionar todas**, presets salváveis, tooltips explicativos, botão **Detalhes** e reversão unitária/total (**Reverter** / **Padrão Win**).
2. **Vigia de Afinidade de Jogos (Nativo em Rust)**:
   - Detecta automaticamente a topologia da CPU (núcleos físicos, threads lógicas e SMT) via Win32 (`GetLogicalProcessorInformationEx`).
   - Mantém jogos e launchers fora do núcleo físico 0 (`0xFE` com SMT OFF / `0xFFFC` com SMT ON) e isola o fundo no núcleo 0 após confirmar uso 3D da GPU (`>= 5%` via PDH Win32), restaurando tudo ao fechar o jogo.
   - Suporta **Minimizar para Bandeja** (ícone próprio na área de notificação) e botão 1-clique **"Iniciar com o Windows"** (tarefa agendada com `RunLevel Highest` sem UAC, iniciando Vigia + ISLC automaticamente 5s após o logon).
3. **Gerenciador de Aplicativos**:
   - Catálogo embutido (`assets\Aplicativos_01LowOS.json`), detecção automática e verificação por assinatura Authenticode ou SHA-256 antes da instalação silenciosa.
4. **Orquestrador de Build de ISO**:
   - Inspeciona pré-requisitos locais e acompanha fases de build de ISO customizada apenas sob ação explícita.
5. **Ferramentas Extras & Backups**:
   - Diagnóstico rápido de saúde gamer (HAGS, VBS, Modo de Jogo, OneDrive, ExitLag, Conta MS), limpador nativo de **Standby List** (estilo ISLC), gerenciador de **CFG por jogo DX12** (com trava anti-Vanguard `VAN9002`), limpador de **Cache de Shaders** e **Temporários / Cache Windows Update**, log persistente (`01LowOS.log`), reversão unitária/total, **Exportar Relatório (.txt)** e auto-teste (`--self-test` / `Executar_AutoTeste_Admin.cmd`).

## Uso via Linha de Comando (CLI)

```powershell
01LowOS.exe                      # Abre a interface gráfica
01LowOS.exe --list               # Lista todas as melhorias disponíveis
01LowOS.exe --status             # Verifica o estado atual no sistema (somente leitura)
01LowOS.exe --apply <id>         # Aplica uma melhoria específica com backup
01LowOS.exe --revert <id>        # Reverte uma melhoria específica pelo backup
01LowOS.exe --reset-default <id> # Restaura para o padrão do Windows sem backup
01LowOS.exe --self-test          # Testa aplicar+reverter melhorias pendentes
01LowOS.exe --self-test-all      # Inclui AppX e Optional Features no auto-teste
01LowOS.exe --watchdog           # Executa o Vigia de Afinidade na bandeja
```

## Como Recompilar do Fonte

Requer Rust (`stable-x86_64-pc-windows-gnu` com MinGW-w64 `windres` no `PATH`, ou `msvc`):

```powershell
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

O executável final será gerado em `target\release\01LowOS.exe` sem dependência de DLLs externas.

## Licença

Distribuído sob a licença **GNU General Public License v3.0 (GPL-3.0)**. Veja o arquivo `LICENSE` para mais detalhes.
