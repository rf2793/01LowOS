use std::collections::HashSet;

use crate::sys::registry::{RegHive, RegValue};
use crate::tweaks::advanced::{
    ActionCenterSsoTweak, AmdDeepSleepAspmOffTweak, AmdShaderCacheTweak, AmdUlpsOffTweak,
    AudioEnhancementsTweak, BcdCleanTimersTweak, CloudflareDnsTweak, ConsumerAppxTweak, DefenderGameExclusionsTweak,
    DefenderScanThrottleTweak, EdgeBackgroundTweak, EtwAutologgerTweak, FirewallOrphansTweak,
    GpuNicMsiModeTweak, HdAudioIdleOffTweak, HypervisorOffTweak, LegacyHardwareIrqTweak,
    MemoryCompressionTweak, MmAgentPrelaunchTweak, NicPowerMgmtTweak, NvidiaDynamicPstateTweak,
    NvidiaHdcpOffTweak, NvidiaTelemetryTweak, NvmeStorageMsiTweak, OptionalFeaturesTweak,
    PnpUnusedDevicesTweak, PowerOverlayBestPerfTweak, PresentMonFpsTweak, ReservedStorageOffTweak,
    SmartScreenAclTweak,
    SystemAppxTweak, TcpNagleTweak, ThirdPartyBloatTweak, UsbAudioMsiModeTweak,
    UsbD3SuspendTweak, UsbDevicePowerMgmtTweak, UsbHubPowerMgmtTweak,
};
use crate::tweaks::network::{
    NetBiosOffTweak, NetQosNicFlowTweak, NetRscUdpLatencyTweak, NetTcpBbr2Tweak, NicBindingsTweak,
    NicInterruptModerationOffTweak, NicLsoUsoOffTweak, NicRssTweak, NicWolEeeTweak,
    WinInetWpadTweak,
};
use crate::tweaks::power::{
    DeviceWakeTweak, HiberfilOffTweak, HybridCpuPcoreTweak, PowerPlanTweak,
};
use crate::tweaks::registry_tweak::{RegOp, RegistryTweak};
use crate::tweaks::service_tweak::{ServiceGroupTweak, ServiceTarget};
use crate::tweaks::ssd_only::SsdOnlyTweak;
use crate::tweaks::task_tweak::{ScheduledTaskGroupTweak, TaskPattern};
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta};

fn dw(v: u32) -> RegValue {
    RegValue::DWord(v)
}
fn sz(v: &str) -> RegValue {
    RegValue::String(v.to_string())
}

pub fn build_catalog() -> Vec<Box<dyn Tweak>> {
    let mut list: Vec<Box<dyn Tweak>> = Vec::with_capacity(115);

    // 1. GPU & LATÊNCIA
    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.hags",
            name: "Ativar HAGS (Agendamento de GPU Acelerado por Hardware)",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Ativa o Hardware-Accelerated GPU Scheduling (HwSchMode=2), transferindo parte do agendamento de filas da GPU para o próprio processador gráfico. Melhora 1% low e reduz overhead de kernel nos núcleos do jogo.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode", dw(2))],
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode", dw(1))],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.mpo_off",
            name: "Desativar MPO (Multi-Plane Overlay)",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o Multi-Plane Overlay (DWM) para evitar flicker, microstutters e quedas de flip independente em jogos (especialmente com múltiplos monitores ou overlays).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Dwm", "OverlayTestMode", dw(5)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Dwm", "OverlayMinFPS", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Dwm", "OverlayTestMode"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Dwm", "OverlayMinFPS"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.windowed_optimizations",
            name: "Otimizações para jogos em janela + VRR em janela",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Ativa SwapEffectUpgradeEnable=1 e VRRUpgradeEnable=1 em UserGpuPreferences para reduzir latência via Flip Model em DX10/11 borderless.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hkcu, r"Software\Microsoft\DirectX\UserGpuPreferences", "DirectXUserGlobalSettings", sz("SwapEffectUpgradeEnable=1;VRRUpgradeEnable=1;"))],
        vec![RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\DirectX\UserGpuPreferences", "DirectXUserGlobalSettings")],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.game_mode",
            name: "Ativar Modo de Jogo e desativar GameDVR / captura em 2º plano",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Ativa o Modo de Jogo (AutoGameModeEnabled=1) e desliga gravação GameDVR/GameBar em HKCU e por política em HKLM (mantém FSO padrão do Windows).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "AutoGameModeEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "AllowAutoGameMode", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "UseNexusForGameBarEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "ShowStartupPanel", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_FSEBehavior", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_FSEBehaviorMode", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_HonorUserFSEBehaviorMode", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_DXGIHonorFSEWindowsCompatible", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_EFSEFeatureFlags", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "HistoricalCaptureEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "UseNexusForGameBarEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\GameBar", "ShowStartupPanel", dw(1)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_Enabled", dw(1)),
            RegOp::delete(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_FSEBehavior"),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_FSEBehaviorMode", dw(2)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_HonorUserFSEBehaviorMode", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_DXGIHonorFSEWindowsCompatible", dw(0)),
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_EFSEFeatureFlags", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "HistoricalCaptureEnabled", dw(0)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.gamebar_presence_off",
            name: "Bloquear GameBarPresenceWriter e GameBar em 2º plano (IFEO)",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Redireciona GameBarPresenceWriter.exe e GameBar.exe para systray.exe, evitando o processo residual da Game Bar (~145 MB RAM) sem afetar Xbox App, Game Pass ou Modo de Jogo. Atenção: o atalho Win+G não abrirá a Game Bar até reverter.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\GameBarPresenceWriter.exe", "Debugger", sz(r"C:\Windows\System32\systray.exe")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\GameBar.exe", "Debugger", sz(r"C:\Windows\System32\systray.exe")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\GameBarPresenceWriter.exe", "Debugger"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\GameBar.exe", "Debugger"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "gpu.mmcss_games",
            name: "Perfil MMCSS padrão validado para jogos",
            category: Category::GpuLatency,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define SystemResponsiveness=20, NetworkThrottlingIndex=10, Scheduling Category=Medium e Priority=2 (evita sufocar threads de rede/sistema).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "SystemResponsiveness", dw(20)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "NetworkThrottlingIndex", dw(10)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games", "Scheduling Category", sz("Medium")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games", "Priority", dw(2)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "SystemResponsiveness", dw(20)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "NetworkThrottlingIndex", dw(10)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games", "Scheduling Category", sz("Medium")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games", "Priority", dw(2)),
        ],
    )));

    list.push(Box::new(AmdShaderCacheTweak::new()));
    list.push(Box::new(AmdUlpsOffTweak::new()));
    list.push(Box::new(AmdDeepSleepAspmOffTweak::new()));
    list.push(Box::new(PresentMonFpsTweak::new()));
    list.push(Box::new(GpuNicMsiModeTweak::new()));
    list.push(Box::new(NvidiaTelemetryTweak::new()));
    list.push(Box::new(NvidiaDynamicPstateTweak::new()));
    list.push(Box::new(NvidiaHdcpOffTweak::new()));
    list.push(Box::new(AudioEnhancementsTweak::new()));

    // 2. SISTEMA & KERNEL
    let mut legacy_ops = Vec::new();
    for (subkey, names) in [
        (r"SYSTEM\CurrentControlSet\Control\Session Manager\Kernel", &["ThreadDpcEnable", "DpcWatchdogPeriod", "DpcTimeout", "GlobalTimerResolutionRequests", "SerializeTimerExpiration"][..]),
        (r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", &["TdrDelay", "TdrDdiDelay"][..]),
        (r"SYSTEM\CurrentControlSet\Control\PnP\Pci", &["DisableInterruptSteering", "ExpressBaseVersion"][..]),
        (r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", &["IoPageLockLimit", "LargePageMinimum"][..]),
        (r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", &["NoLazyMode", "AlwaysOn"][..]),
        (r"SYSTEM\CurrentControlSet\Services\stornvme\Parameters\Device", &["IdleTimeout"][..]),
    ] {
        for &n in names {
            legacy_ops.push(RegOp::delete(RegHive::Hklm, subkey, n));
        }
    }
    for guid in [
        "0cc5b647-c1df-4637-891a-dec49c1b8008",
        "ea062107-5d60-478f-a426-6237c69a31f0",
        "0cc5b647-c362-456e-be6e-72589999712f",
        "36687f9e-1337-42ce-b3c0-8ceb65287f34",
        "5d76a2ca-e8c0-402f-a133-2158492d5762",
    ] {
        let subkey = format!(r"SYSTEM\CurrentControlSet\Control\Power\PowerSettings\54533251-82be-4824-96c1-47b60b740d00\{guid}");
        for n in ["ValueMin", "ValueMax", "Attributes"] {
            legacy_ops.push(RegOp::delete(RegHive::Hklm, &subkey, n));
        }
    }
    list.push(Box::new(RegistryTweak::new(
        TweakMeta {
            id: "sys.legacy_cleanup_reg",
            name: "Limpeza: Remover tweaks tóxicos/inventados antigos do Registro",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Remove chaves obsoletas ou tóxicas de versões legadas (GlobalTimerResolutionRequests, TdrDelay, IoPageLockLimit, NoLazyMode, stornvme\\IdleTimeout e PowerSettings inventados).",
            details: vec![],
        },
        legacy_ops,
    )));

    list.push(Box::new(LegacyHardwareIrqTweak::new()));
    list.push(Box::new(BcdCleanTimersTweak::new()));
    list.push(Box::new(UsbAudioMsiModeTweak::new()));
    list.push(Box::new(NvmeStorageMsiTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.fast_startup_off",
            name: "Desativar Inicialização Rápida e Hibernação",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define HiberbootEnabled=0, HibernateEnabled=0 e HibernateEnabledDefault=0 para garantir boot frio limpo e recarga real das políticas dos drivers.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabledDefault", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabledDefault", dw(1)),
        ],
    )));

    list.push(Box::new(MemoryCompressionTweak::new()));
    list.push(Box::new(MmAgentPrelaunchTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.vbs_hvci_off",
            name: "Desativar VBS / HVCI / Credential Guard",
            category: Category::SystemKernel,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desliga Virtualization-Based Security (EnableVirtualizationBasedSecurity=0), Memory Integrity (HVCI Enabled=0) e LsaCfgFlags=0 para eliminar overhead de hipervisor nos jogos.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\DeviceGuard", "EnableVirtualizationBasedSecurity", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity", "Enabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Lsa", "LsaCfgFlags", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\DeviceGuard", "EnableVirtualizationBasedSecurity", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity", "Enabled", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Lsa", "LsaCfgFlags"),
        ],
    )));

    list.push(Box::new(HypervisorOffTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.cpu_mitigations_off",
            name: "CPU: Desativar mitigações Spectre/Meltdown (FeatureSettingsOverride=3)",
            category: Category::SystemKernel,
            risk: RiskLevel::High,
            requires_reboot: true,
            requires_admin: true,
            description: "Define FeatureSettingsOverride=3 e FeatureSettingsOverrideMask=3 em Memory Management (mitigacoes Spectre v2/Meltdown). So faz diferenca em CPUs antigas (Intel ate ~9a geracao). Medicao do Phoronix: no Zen 5 (Ryzen 9000, inclusive 9800X3D) desligar as mitigacoes nao deu ganho; no Zen 4 o sistema foi ~3% mais rapido COM elas. Em AMD recente so remove protecao. Atencao: desativa protecoes contra vulnerabilidades de hardware.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", "FeatureSettingsOverride", dw(3)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", "FeatureSettingsOverrideMask", dw(3)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", "FeatureSettingsOverride"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", "FeatureSettingsOverrideMask"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.fth_off",
            name: "Desativar Fault Tolerant Heap (FTH)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Impede que o Windows aplique shim permanente de heap (com custo de desempenho) após crashes repetidos de um jogo.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\FTH", "Enabled", dw(0))],
        vec![RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\FTH", "Enabled", dw(1))],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.crash_rsop_clean",
            name: "Sistema: Desativar AutoReboot, RSoP logging e ClearPageFileAtShutdown",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Define CrashDumpEnabled=3 (minidump), AutoReboot=0, RSoPLogging=0 e ClearPageFileAtShutdown=0, evitando overhead de políticas RSoP em segundo plano e desligamentos lentos.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\CrashControl", "CrashDumpEnabled", dw(3)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\CrashControl", "AutoReboot", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "RSoPLogging", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management", "ClearPageFileAtShutdown", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\CrashControl", "AutoReboot", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "RSoPLogging"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.power_throttling_off",
            name: "Desativar Power Throttling (QoS do Windows 11)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define PowerThrottlingOff=1 para impedir que o agendador QoS do Windows 11 estrangule jogos em janela sem borda, overlays ou processos auxiliares.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power\PowerThrottling", "PowerThrottlingOff", dw(1))],
        vec![RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power\PowerThrottling", "PowerThrottlingOff")],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.win32_priority_separation",
            name: "Agendador CPU: Win32PrioritySeparation=0x26 (Short, Variable, High Foreground)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Bitmask AABBCC da Microsoft: 0x26 (38) = quantum curto (10), variavel (01) e reforco alto 3:1 para o processo em primeiro plano (10). O reforco so vale com quantum variavel. Sem benchmark publicado de ganho em FPS ou 1% low; o padrao do Windows cliente e 0x2.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\PriorityControl", "Win32PrioritySeparation", dw(0x26))],
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\PriorityControl", "Win32PrioritySeparation", dw(2))],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.ntfs_low_overhead",
            name: "NTFS: Desativar LastAccessUpdate, nomes 8.3 e criptografia de PageFile (mantém TRIM)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Evita gravações de metadata a cada leitura de arquivos/texturas nos jogos (fsutil disablelastaccess 1 / 0x80000001, disable8dot3 1 e NtfsEncryptPagingFile=0) sem desativar TRIM.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisableLastAccessUpdate", dw(0x8000_0001)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisable8dot3NameCreation", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsEncryptPagingFile", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisableLastAccessUpdate", dw(0x8000_0002)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisable8dot3NameCreation", dw(2)),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsEncryptPagingFile"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.ntfs_memory_usage",
            name: "NTFS: Aumentar cache de metadados em RAM (MemoryUsage=2)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define NtfsMemoryUsage=2 (equivalente a fsutil behavior set memoryusage 2) para ampliar o pool paginado de metadados NTFS em PCs com 16–32 GB+.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsMemoryUsage", dw(2))],
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsMemoryUsage", dw(0))],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.storage_trim_on",
            name: "SSD/NVMe: Garantir TRIM ativo em NTFS e ReFS (DisableDeleteNotify=0)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Garante DisableDeleteNotify=0 e RefsDisableDeleteNotify=0, mantendo o TRIM ativo no SSD/NVMe para evitar degradação de escrita e micro-travamentos em I/O.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "DisableDeleteNotify", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "RefsDisableDeleteNotify", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "DisableDeleteNotify", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "RefsDisableDeleteNotify", dw(0)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.bitlocker_auto_encrypt_off",
            name: "SSD/NVMe: Impedir criptografia automática BitLocker (PreventDeviceEncryption=1)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Define Control\\BitLocker\\PreventDeviceEncryption=1 para impedir que o Windows 11 24H2/25H2 ative criptografia por software BitLocker automaticamente em unidades NVMe/SSD (que pode reduzir em 20–45% o desempenho 4K aleatório e carregamento de texturas). Nota: não descriptografa unidades já criptografadas.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\BitLocker", "PreventDeviceEncryption", dw(1))],
        vec![RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\BitLocker", "PreventDeviceEncryption")],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.wpbt_off",
            name: "BIOS/Firmware: Bloquear execução OEM via WPBT (DisableWpbtExecution=1)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define Session Manager\\DisableWpbtExecution=1 para impedir que a tabela ACPI WPBT da placa-mãe (ex.: ASUS Armoury Crate/DriverHub, Gigabyte, MSI) injete e execute instaladores OEM a cada boot.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager", "DisableWpbtExecution", dw(1))],
        vec![RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager", "DisableWpbtExecution")],
    )));

    // PC-Tuning: so desativar Prefetch/Superfetch quando nao ha HDD (SsdOnlyTweak vira N/A com HDD).
    list.push(Box::new(SsdOnlyTweak::new(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.prefetcher_off",
            name: "NVMe/SSD: Desativar Prefetcher e Superfetch (EnablePrefetcher=0)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Define EnablePrefetcher=0 e EnableSuperfetch=0 em PrefetchParameters, evitando gravacoes de arquivos .pf no NVMe/SSD sem beneficio em armazenamento flash. Fica N/A se houver HD mecanico, onde o Prefetch acelera o carregamento.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters", "EnablePrefetcher", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters", "EnableSuperfetch", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters", "EnablePrefetcher", dw(3)),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters", "EnableSuperfetch"),
        ],
    )))));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.uac_never_notify",
            name: "UAC no nível 'Nunca notificar'",
            category: Category::SystemKernel,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Define ConsentPromptBehaviorAdmin=0 e PromptOnSecureDesktop=0 para que elevações ocorram sem escurecer a tela nem travar jogos em tela cheia. Atenção: remove os avisos de confirmação do UAC.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "ConsentPromptBehaviorAdmin", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "PromptOnSecureDesktop", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "ConsentPromptBehaviorAdmin", dw(5)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "PromptOnSecureDesktop", dw(1)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.smartscreen_off",
            name: "Desativar SmartScreen do Explorer por política",
            category: Category::SystemKernel,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Define EnableSmartScreen=0 em Policies\\Microsoft\\Windows\\System para suprimir 'Windows protected your PC' mantendo o Attachment Manager padrão.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableSmartScreen", dw(0))],
        vec![RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableSmartScreen")],
    )));

    list.push(Box::new(SmartScreenAclTweak::new()));

    // Complemento do SmartScreen OFF (decisao do usuario, 20/10/2026): Protecao Avancada contra
    // Phishing (politica oficial WebThreatDefense/ServiceEnabled, Policy CSP no Microsoft Learn) e
    // SmartScreen de conteudo web dos apps da Store (interruptor do Windows Security).
    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.phishing_store_smartscreen_off",
            name: "SmartScreen: Desativar Protecao Avancada contra Phishing e verificacao de apps da Store",
            category: Category::SystemKernel,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Politica WTDS\\Components ServiceEnabled=0 (Policy CSP WebThreatDefense, desliga a captura de digitacao de senha do Enhanced Phishing Protection) e AppHost EnableWebContentEvaluation=0 (SmartScreen de URLs usadas por apps da Store). Complementa sys.smartscreen_off; reduz a protecao contra phishing.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WTDS\Components", "ServiceEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\AppHost", "EnableWebContentEvaluation", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WTDS\Components", "ServiceEnabled"),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\AppHost", "EnableWebContentEvaluation"),
        ],
    )));

    list.push(Box::new(DefenderGameExclusionsTweak::new()));
    list.push(Box::new(DefenderScanThrottleTweak::new()));
    list.push(Box::new(ReservedStorageOffTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "sys.ifeo_touch_voice_off",
            name: "Bloquear teclado virtual (TabTip) e VoiceAccess por IFEO",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Redireciona TabTip.exe (~70 MB no logon) e VoiceAccess.exe (Win+Ctrl+S) para systray.exe evitando janelas sobre o jogo.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\TabTip.exe", "Debugger", sz(r"C:\Windows\System32\systray.exe")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\VoiceAccess.exe", "Debugger", sz(r"C:\Windows\System32\systray.exe")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\TabTip.exe", "Debugger"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\VoiceAccess.exe", "Debugger"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "sys.bluetooth_auto",
            name: "Bluetooth: Iniciar serviço bthserv automaticamente",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Define bthserv Start=2 (Automático) para que controles sem fio Bluetooth conectem imediatamente sem precisar abrir Configurações.",
            details: vec![],
        },
        vec![ServiceTarget::auto("bthserv")],
    )));

    // 3. ENERGIA
    list.push(Box::new(PowerPlanTweak::new()));
    list.push(Box::new(PowerOverlayBestPerfTweak::new()));
    list.push(Box::new(HiberfilOffTweak::new()));
    list.push(Box::new(DeviceWakeTweak::new()));
    list.push(Box::new(UsbDevicePowerMgmtTweak::new()));
    list.push(Box::new(UsbHubPowerMgmtTweak::new()));
    list.push(Box::new(UsbD3SuspendTweak::new()));
    list.push(Box::new(HdAudioIdleOffTweak::new()));
    list.push(Box::new(HybridCpuPcoreTweak::new()));

    // 4. REDE
    list.push(Box::new(NicRssTweak::new()));
    list.push(Box::new(NicWolEeeTweak::new()));
    list.push(Box::new(NicPowerMgmtTweak::new()));
    list.push(Box::new(NicBindingsTweak::new()));
    list.push(Box::new(TcpNagleTweak::new()));
    list.push(Box::new(NetRscUdpLatencyTweak::new()));
    list.push(Box::new(NetQosNicFlowTweak::new()));
    list.push(Box::new(NicInterruptModerationOffTweak::new()));
    list.push(Box::new(NicLsoUsoOffTweak::new()));
    list.push(Box::new(NetTcpBbr2Tweak::new()));
    list.push(Box::new(NetBiosOffTweak::new()));
    list.push(Box::new(CloudflareDnsTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "net.llmnr_mdns_wpad_off",
            name: "Rede: Desativar LLMNR, mDNS, WPAD WinHTTP, Negative DNS Cache e túneis IPv6",
            category: Category::Network,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa protocolos de descoberta multicast/anúncio local, túneis IPv6 de transição e zera MaxNegativeCacheTtl (evita cache de falhas DNS em reconexões/matchmaking).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient", "EnableMulticast", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WinHttp", "DisableWpad", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "EnableMDNS", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "MaxNegativeCacheTtl", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "6to4_State", sz("Disabled")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "ISATAP_State", sz("Disabled")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "Teredo_State", sz("Disabled")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient", "EnableMulticast"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WinHttp", "DisableWpad"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "EnableMDNS"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "MaxNegativeCacheTtl"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "6to4_State"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "ISATAP_State"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "Teredo_State"),
        ],
    )));

    list.push(Box::new(WinInetWpadTweak::new()));

    // 5. USUÁRIO (HKCU)
    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.mouse_raw",
            name: "Mouse RAW 1:1 (sem aceleração nem atraso)",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Zera MouseSpeed, MouseThreshold1/2, MouseHoverTime, MouseDelay e grava curvas SmoothMouse lineares no perfil do usuário.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseSpeed", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseThreshold1", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseThreshold2", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseSensitivity", sz("10")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseHoverTime", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseTrails", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseDelay", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "SmoothMouseXCurve", RegValue::Binary(vec![
                0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00, 0x15,0x6e,0x00,0x00,0x00,0x00,0x00,0x00,
                0x00,0x40,0x01,0x00,0x00,0x00,0x00,0x00, 0x00,0x80,0x02,0x00,0x00,0x00,0x00,0x00,
                0x00,0x00,0x05,0x00,0x00,0x00,0x00,0x00,
            ])),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "SmoothMouseYCurve", RegValue::Binary(vec![
                0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00, 0xfd,0x11,0x01,0x00,0x00,0x00,0x00,0x00,
                0x00,0x24,0x04,0x00,0x00,0x00,0x00,0x00, 0x00,0xfc,0x12,0x00,0x00,0x00,0x00,0x00,
                0x00,0xc0,0xbb,0x01,0x00,0x00,0x00,0x00,
            ])),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseSpeed", sz("1")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseThreshold1", sz("6")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseThreshold2", sz("10")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseSensitivity", sz("10")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseHoverTime", sz("400")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseTrails", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Mouse", "MouseDelay", sz("0")),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.keyboard_accessibility",
            name: "Teclado rápido + Desativar atalhos de acessibilidade (5x Shift/HighContrast/MouseKeys) e Snipping no Print",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Define KeyboardDelay=0, KeyboardSpeed=31, NumLock ligado, desliga gatilhos de StickyKeys/FilterKeys/ToggleKeys, HighContrast (Alt+Shift+PrintScreen) e MouseKeys (Alt+Shift+NumLock) que tiram o jogo de tela cheia e volta PrintScreen ao print clássico.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "KeyboardDelay", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "KeyboardSpeed", sz("31")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "InitialKeyboardIndicators", sz("2")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "PrintScreenKeyForSnippingEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\StickyKeys", "Flags", sz("506")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\Keyboard Response", "Flags", sz("122")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\ToggleKeys", "Flags", sz("58")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\HighContrast", "Flags", sz("122")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\MouseKeys", "Flags", sz("58")),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "KeyboardDelay", sz("1")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "PrintScreenKeyForSnippingEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\StickyKeys", "Flags", sz("510")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\Keyboard Response", "Flags", sz("126")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\ToggleKeys", "Flags", sz("62")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\HighContrast", "Flags", sz("126")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\MouseKeys", "Flags", sz("62")),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.visual_perf",
            name: "Efeitos Visuais de Desempenho (Transparência/Animações/RGB Dinâmico OFF)",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Define VisualFXSetting=2, desliga transparência DWM, animações de janela, Iluminação Dinâmica (evita conflito RGB) e zera MenuShowDelay.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "MenuShowDelay", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "WaitToKillAppTimeout", sz("5000")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "HungAppTimeout", sz("5000")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "LowLevelHooksTimeout", sz("1000")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "AutoEndTasks", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "FontSmoothing", sz("2")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "UserPreferencesMask", RegValue::Binary(vec![0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00])),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop\WindowMetrics", "MinAnimate", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Lighting", "AmbientLightingEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", dw(0)),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "MenuShowDelay", sz("400")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop\WindowMetrics", "MinAnimate", sz("1")),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Lighting", "AmbientLightingEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", dw(1)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.explorer_clean",
            name: "Explorer e Barra de Tarefas: Menu clássico, extensões visíveis e End Task",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Ativa menu de contexto clássico do Win11, mostra extensões e ocultos, remove Search/Copilot/TaskView/Snap da barra e habilita Finalizar Tarefa no clique direito.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32", "", sz("")),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "HideFileExt", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Hidden", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarMn", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowTaskViewButton", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowCopilotButton", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "DisallowShaking", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "EnableSnapAssistFlyout", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "EnableSnapBar", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_TrackProgs", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_TrackDocs", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_AccountNotifications", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_IrisRecommendations", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowSyncProviderNotifications", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "SearchboxTaskbarMode", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "BackgroundAppGlobalToggle", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\SearchSettings", "IsDynamicSearchBoxEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\SearchSettings", "IsDeviceSearchHistoryEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\Bags\AllFolders\Shell", "FolderType", sz("NotSpecified")),
        ],
        vec![
            RegOp::delete(RegHive::Hkcu, r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32", ""),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "HideFileExt", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Hidden", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_TrackDocs", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_AccountNotifications", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\SearchSettings", "IsDynamicSearchBoxEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\SearchSettings", "IsDeviceSearchHistoryEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowTaskViewButton", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", dw(0)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.explorer_gallery_quickaccess_clean",
            name: "Explorer: Ocultar Galeria, abrir em Este Computador e limpar Recentes/Frequentes",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Oculta a visualização Galeria do Explorer, abre janelas direto em 'Este Computador' (LaunchTo=1) e desativa pastas frequentes e histórico de documentos recentes.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Classes\CLSID\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}", "System.IsPinnedToNameSpaceTree", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "LaunchTo", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowFrequent", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowRecent", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoRecentDocsHistory", dw(1)),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Classes\CLSID\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}", "System.IsPinnedToNameSpaceTree", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "LaunchTo", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowFrequent", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer", "ShowRecent", dw(1)),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoRecentDocsHistory"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.desktop_drag_watermark",
            name: "Estética: Arrastar só contorno (DragFullWindows=0) e versão na área de trabalho",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Ajustes opcionais de preferência pessoal: mostra apenas a borda ao arrastar janelas e exibe a build do Windows no canto inferior direito.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "DragFullWindows", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "PaintDesktopVersion", dw(1)),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "DragFullWindows", sz("1")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "PaintDesktopVersion", dw(0)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.audio_no_ducking",
            name: "Áudio: Impedir Windows de baixar o volume dos jogos em chamadas",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Define UserDuckingPreference=3 (Não fazer nada) para que Discord ou chamadas não reduzam o som do jogo.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Multimedia\Audio", "UserDuckingPreference", dw(3))],
        vec![RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Multimedia\Audio", "UserDuckingPreference", dw(2))],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.privacy_hkcu",
            name: "Privacidade HKCU: Sem apps em 2º plano, sugestões, anúncios, histórico de clipboard e proteção de tela",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Desliga ContentDeliveryManager, anúncios, apps em segundo plano, histórico Win+V (mantém copiar/colar e recorte) e proteção de tela.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Clipboard", "EnableClipboardHistory", dw(0)),
            RegOp::set(RegHive::Hkcu, r"SOFTWARE\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications", "GlobalUserDisabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "ScreenSaveActive", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "ScreenSaverIsSecure", sz("0")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Desktop", "ScreenSaveTimeOut", sz("0")),
            RegOp::delete(RegHive::Hkcu, r"Control Panel\Desktop", "SCRNSAVE.EXE"),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "ContentDeliveryAllowed", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "FeatureManagementEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "OemPreInstalledAppsEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "PreInstalledAppsEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SilentInstalledAppsEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SoftLandingEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SystemPaneSuggestionsEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "RotatingLockScreenEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "RotatingLockScreenOverlayEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-310093Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-338387Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-338388Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-338389Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-338393Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-353694Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SubscribedContent-353696Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement", "ScoobeSystemSettingEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\Windows.SystemToast.Suggested", "Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings", "NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Input\TIPC", "Enabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Privacy", "TailoredExperiencesWithDiagnosticDataEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Personalization\Settings", "AcceptedPrivacyPolicy", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\International\User Profile", "HttpAcceptLanguageOptOut", dw(1)),
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Clipboard", "EnableClipboardHistory", dw(1)),
            RegOp::set(RegHive::Hkcu, r"SOFTWARE\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications", "GlobalUserDisabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "ContentDeliveryAllowed", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "FeatureManagementEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "SystemPaneSuggestionsEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "RotatingLockScreenEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager", "RotatingLockScreenOverlayEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement", "ScoobeSystemSettingEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Input\TIPC", "Enabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Privacy", "TailoredExperiencesWithDiagnosticDataEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Personalization\Settings", "AcceptedPrivacyPolicy", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\International\User Profile", "HttpAcceptLanguageOptOut", dw(0)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.nearby_speech_siuf_off",
            name: "Privacidade Win11 (HKCU): Sem Feedback SIUF, Fala Online, Nearby Sharing, Home Settings e Recomendações",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Desativa pedidos de feedback (SIUF=0), reconhecimento de fala online, coleta de contatos para digitação, Compartilhamento por Proximidade/CDP, diagnóstico de apps, recomendações do Menu Iniciar e a aba promocional 'Início' das Configurações (Win11 24H2/25H2).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Siuf\Rules", "NumberOfSIUFInPeriod", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy", "HasAccepted", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CDP", "CdpSessionUserAuthzPolicy", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CDP", "NearShareChannelUserAuthzPolicy", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\appDiagnostics", "Value", sz("Deny")),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_Recommended", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "SettingsPageVisibility", sz("hide:home")),
        ],
        vec![
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Siuf\Rules", "NumberOfSIUFInPeriod"),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy", "HasAccepted", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CDP", "CdpSessionUserAuthzPolicy", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CDP", "NearShareChannelUserAuthzPolicy", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\appDiagnostics", "Value", sz("Allow")),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_Recommended", dw(1)),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "SettingsPageVisibility"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.sound_scheme_none",
            name: "Áudio: Esquema de sons do Windows em Sem Som (.None)",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Define o esquema de sons do sistema como .None, evitando disparos de áudio de notificação do Windows pelo audiodg.exe durante jogos (100% reversível).",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hkcu, r"AppEvents\Schemes", "", sz(".None"))],
        vec![RegOp::set(RegHive::Hkcu, r"AppEvents\Schemes", "", sz(".Default"))],
    )));

    // 6. PRIVACIDADE & BLOAT
    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.wu_policies",
            name: "Políticas: Bloquear Windows Update automático, busca de drivers via WU e Telemetria",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa Windows Update automático, busca de drivers pelo WU e telemetria (AllowTelemetry=0, DODownloadMode=99), evitando downloads/reinícios em jogo. Atenção: atualizações passam a ser apenas manuais.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU", "NoAutoUpdate", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "SetDisableUXWUAccess", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "ExcludeWUDriversInQualityUpdate", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\DriverSearching", "SearchOrderConfig", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "SearchOrderConfig", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "DontSearchWindowsUpdate", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "DriverUpdateWizardWuSearchEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Device Metadata", "PreventDeviceMetadataFromNetwork", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Device Metadata", "PreventDeviceMetadataFromNetwork", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode", dw(99)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection", "AllowTelemetry", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU", "NoAutoUpdate"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "SetDisableUXWUAccess"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "ExcludeWUDriversInQualityUpdate"),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\DriverSearching", "SearchOrderConfig", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "SearchOrderConfig"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "DontSearchWindowsUpdate"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DriverSearching", "DriverUpdateWizardWuSearchEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Device Metadata", "PreventDeviceMetadataFromNetwork"),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Device Metadata", "PreventDeviceMetadataFromNetwork", dw(0)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection", "AllowTelemetry"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.ai_search_cloud",
            name: "Políticas: Desativar Copilot, Recall/IA, Widgets, SearchHost e CloudContent",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desliga Copilot (HKLM+HKCU), Recall/snapshots de IA, ClickToDo, Widgets da barra de tarefas (~150–300 MB), SearchHost+WebView2, Cortana, CloudContent, LockScreen e Dicas Online. Nota: no Win11 24H2/25H2, as 2 chaves de Widgets podem exigir aplicar 'bloat.ucpd_sppsvc' + reiniciar antes.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreen", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableWindowsConsumerFeatures", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableSoftLanding", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableCloudOptimizedContent", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableRecall", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "TurnOffSavingSnapshots", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableClickToDo", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableWindowsSpotlightFeatures", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableAIImageGeneration", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableCocreator", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableGenerativeFill", dw(1)),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "AllowNewsAndInterests", dw(0)),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "DisableWidgetsOnLockScreen", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Feeds", "EnableFeeds", dw(0)),
            RegOp::set_opt(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebSearch", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebViewSearch", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableSearch", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "AllowOnlineTips", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Explorer", "NoUseStoreOpenWith", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DisableDiagnosticDataViewer", dw(1)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreen"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableWindowsConsumerFeatures"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableSoftLanding"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableCloudOptimizedContent"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableRecall"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "TurnOffSavingSnapshots"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableClickToDo"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableWindowsSpotlightFeatures"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableAIImageGeneration"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableCocreator"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Paint", "DisableGenerativeFill"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "AllowNewsAndInterests"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "DisableWidgetsOnLockScreen"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Feeds", "EnableFeeds"),
            RegOp::set_opt(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa", dw(1)),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebViewSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "AllowOnlineTips"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Explorer", "NoUseStoreOpenWith"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DisableDiagnosticDataViewer"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.store_autoupdate_off",
            name: "Microsoft Store: Desativar downloads automáticos em 2º plano (AutoDownload=2)",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Define Policies\\Microsoft\\WindowsStore\\AutoDownload=2 para impedir que a Microsoft Store baixe e descompacte atualizações de apps silenciosamente durante partidas (evitando picos de ping, CPU e disco). A loja, Xbox App e Game Pass continuam 100% funcionais para downloads manuais.",
            details: vec![],
        },
        vec![RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\WindowsStore", "AutoDownload", dw(2))],
        vec![RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\WindowsStore", "AutoDownload")],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.appcompat_sqm_telemetry",
            name: "Políticas GPO: Desativar AppCompat, CEIP/SQM, Dumps de Diagnóstico e Auto-Restart",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Aplica políticas oficiais GPO da Microsoft para desligar telemetria AppCompat (AIT/PCA/Steps Recorder), CEIP/SQM, coleta de logs/dumps de diagnóstico, OneSettings, escrita à mão, Histórico de Arquivos, tráfego de Mapas e reabertura automática pós-update.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "AITEnable", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisableInventory", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisablePCA", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisableUAR", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\SQMClient\Windows", "CEIPEnable", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "LimitDiagnosticLogCollection", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "LimitDumpCollection", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DisableOneSettingsDownloads", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\HandwritingErrorReports", "PreventHandwritingErrorReports", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TabletPC", "PreventHandwritingDataSharing", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\FileHistory", "Disabled", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Maps", "AutoDownloadAndUpdateMapData", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Maps", "AllowUntriggeredNetworkTrafficOnSettingsPage", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "DisableAutomaticRestartSignOn", dw(1)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "AITEnable"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisableInventory"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisablePCA"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppCompat", "DisableUAR"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\SQMClient\Windows", "CEIPEnable"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "LimitDiagnosticLogCollection"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "LimitDumpCollection"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DisableOneSettingsDownloads"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\HandwritingErrorReports", "PreventHandwritingErrorReports"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TabletPC", "PreventHandwritingDataSharing"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\FileHistory", "Disabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Maps", "AutoDownloadAndUpdateMapData"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Maps", "AllowUntriggeredNetworkTrafficOnSettingsPage"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "DisableAutomaticRestartSignOn"),
        ],
    )));

    list.push(Box::new(EdgeBackgroundTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.browsers_bg_telemetry_off",
            name: "Navegadores (Chrome, Brave, Edge, Firefox): Desativar fundo após fechar e telemetria",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Impede Chrome e Brave de continuarem rodando na bandeja após fechar (BackgroundModeEnabled=0) e desativa telemetria/estudos/promoções/coleções de Chrome, Brave, Edge e Firefox (DisableTelemetry, DisableFirefoxStudies, DisablePocket) por política oficial.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "BackgroundModeEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "MetricsReportingEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "PromotionsEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\BraveSoftware\Brave", "BackgroundModeEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\BraveSoftware\Brave", "MetricsReportingEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "MetricsReportingEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "DiagnosticData", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeCollectionsEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "ShowRecommendationsEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "PersonalizationReportingEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "ShowMicrosoftRewards", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "SpotlightExperiencesAndRecommendationsEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeFollowEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisableTelemetry", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisableFirefoxStudies", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisablePocket", dw(1)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "BackgroundModeEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "MetricsReportingEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Google\Chrome", "PromotionsEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\BraveSoftware\Brave", "BackgroundModeEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\BraveSoftware\Brave", "MetricsReportingEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "MetricsReportingEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "DiagnosticData"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeCollectionsEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "ShowRecommendationsEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "PersonalizationReportingEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "ShowMicrosoftRewards"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "SpotlightExperiencesAndRecommendationsEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeFollowEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisableTelemetry"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisableFirefoxStudies"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Mozilla\Firefox", "DisablePocket"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.autorun_off",
            name: "Segurança & Foco: Desativar AutoRun e AutoPlay em todas as unidades",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Define NoDriveTypeAutoRun=255 (HKLM/HKCU) e DisableAutoplay=1, evitando popups de reprodução automática e roubo de foco ao conectar pendrives ou periféricos USB durante jogos.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoDriveTypeAutoRun", dw(255)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoDriveTypeAutoRun", dw(255)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers", "DisableAutoplay", dw(1)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoDriveTypeAutoRun"),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoDriveTypeAutoRun"),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers", "DisableAutoplay", dw(0)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.cli_notepad_ai_telemetry_off",
            name: "Políticas: Desativar IA no Bloco de Notas, FindMyDevice e telemetria CLI (.NET/PS)",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa recursos de IA no Bloco de Notas (Win11 24H2+), banners de conta no CloudContent, FindMyDevice passivo e telemetria oficial de CLI (.NET e PowerShell).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\WindowsNotepad", "DisableAIFeatures", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableConsumerAccountStateContent", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\FindMyDevice", "AllowFindMyDevice", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "DOTNET_CLI_TELEMETRY_OPTOUT", sz("1")),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "POWERSHELL_TELEMETRY_OPTOUT", sz("1")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\WindowsNotepad", "DisableAIFeatures"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableConsumerAccountStateContent"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\FindMyDevice", "AllowFindMyDevice"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "DOTNET_CLI_TELEMETRY_OPTOUT"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "POWERSHELL_TELEMETRY_OPTOUT"),
        ],
    )));

    // Policy CSP TextInput/AllowLinguisticDataCollection (Microsoft Learn) + Configuracoes >
    // Digitacao > "Insights de digitacao" (HKCU input\Settings). Coleta sem utilidade para jogar.
    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.typing_data_off",
            name: "Privacidade: Sem envio de dados de digitacao/tinta e sem Insights de digitacao",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Politica AllowLinguisticDataCollection=0 (nao envia amostras de digitacao e tinta para melhorar o reconhecimento) e InsightsEnabled=0 (desliga as estatisticas de digitacao do teclado virtual/sugestoes).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\TextInput", "AllowLinguisticDataCollection", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\input\Settings", "InsightsEnabled", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\TextInput", "AllowLinguisticDataCollection"),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\input\Settings", "InsightsEnabled"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.office_telemetry_off",
            name: "Políticas Office/Microsoft 365: Desativar telemetria, feedback e análise de conteúdo",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Aplica políticas oficiais GPO do Microsoft Office 16.0 / Microsoft 365 em HKCU para desativar telemetria de cliente, prompts de feedback e experiências conectadas opcionais/análise de conteúdo, preservando login da Conta Microsoft e salvamento no OneDrive.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\clienttelemetry", "DisableTelemetry", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\privacy", "controllerconnectedservicesenabled", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\privacy", "usercontentdisabled", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\feedback", "enabled", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\clienttelemetry", "DisableTelemetry"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\privacy", "controllerconnectedservicesenabled"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\privacy", "usercontentdisabled"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\office\16.0\common\feedback", "enabled"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.lockscreen_logon_clean",
            name: "Políticas GPO: Desativar Acrylic no logon, Câmera/Slideshow na tela de bloqueio e AppPrivacy extra",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Aplica políticas oficiais GPO da Microsoft para remover blur Acrylic na tela de logon, desativar câmera/slideshow na tela de bloqueio e impedir apps UWP de ler diagnóstico, histórico de chamadas, mensagens e sincronização em 2º plano (preserva câmera normal após login, microfone, Conta Microsoft, Xbox e notificações).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "DisableAcrylicBackgroundOnLogon", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreenCamera", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreenSlideshow", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessDiagnosticInfo", dw(2)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsSyncWithDevices", dw(2)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessCallHistory", dw(2)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessMessaging", dw(2)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "DisableAcrylicBackgroundOnLogon"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreenCamera"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreenSlideshow"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessDiagnosticInfo"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsSyncWithDevices"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessCallHistory"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsAccessMessaging"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.telemetry_restore_wer",
            name: "Políticas: Desativar System Restore, WER, Localização, Sync, Ink e Shell órfãos",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa snapshots VSS do System Restore, Relatório de Erros, Histórico de Atividades, Anúncios, Localização, Sync, Assistência Remota e bloqueia extensões órfãs do clique direito (preserva Conta Microsoft, Windows Hello/PIN e OneDrive/Arquivos na Nuvem).",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore", "DisableSR", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "AllowClipboardHistory", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "AllowCrossDeviceClipboard", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Error Reporting", "Disabled", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Windows Error Reporting", "Disabled", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Remote Assistance", "fAllowToGetHelp", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Remote Assistance", "fAllowFullControl", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\SystemCertificates\AuthRoot", "EnableDisallowedCertAutoUpdate", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableActivityFeed", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "UploadUserActivities", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo", "DisabledByGroupPolicy", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocationScripting", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableWindowsLocationProvider", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\SettingSync", "DisableSettingSync", dw(2)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\SettingSync", "DisableSettingSyncUserOverride", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\WindowsInkWorkspace", "AllowWindowsInkWorkspace", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableCdp", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableFontProviders", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DoNotShowFeedbackNotifications", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection", "MaxTelemetryAllowed", dw(0)),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{596AB062-B4D2-4215-9F74-E9109B0A8153}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{7AD84985-87B4-4a16-BE58-8B72A5B390F7}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{f81e9010-6ea4-11ce-a7ff-00aa003ca9f6}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{e2bf9676-5f8f-435c-97eb-11607a5bedf7}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{3dad6c5d-2167-4cae-9914-f99e41c12cfa}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{09A47860-11B0-4DA5-AFA5-26D86198A780}", sz("")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore", "DisableSR"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "AllowClipboardHistory"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "AllowCrossDeviceClipboard"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Error Reporting", "Disabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\Windows Error Reporting", "Disabled"),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Remote Assistance", "fAllowToGetHelp", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Remote Assistance", "fAllowFullControl", dw(0)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\SystemCertificates\AuthRoot", "EnableDisallowedCertAutoUpdate"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableActivityFeed"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "UploadUserActivities"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo", "DisabledByGroupPolicy"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocationScripting"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableWindowsLocationProvider"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\SettingSync", "DisableSettingSync"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\SettingSync", "DisableSettingSyncUserOverride"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\WindowsInkWorkspace", "AllowWindowsInkWorkspace"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableCdp"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableFontProviders"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DoNotShowFeedbackNotifications"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection", "MaxTelemetryAllowed"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{596AB062-B4D2-4215-9F74-E9109B0A8153}"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{7AD84985-87B4-4a16-BE58-8B72A5B390F7}"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{f81e9010-6ea4-11ce-a7ff-00aa003ca9f6}"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{e2bf9676-5f8f-435c-97eb-11607a5bedf7}"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{3dad6c5d-2167-4cae-9914-f99e41c12cfa}"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{09A47860-11B0-4DA5-AFA5-26D86198A780}"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.maintenance_background_off",
            name: "Manutenção Automática, Storage Sense, CrossDevice, AppPrivacy e SRUM OFF",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa Manutenção Automática em 2º plano, Storage Sense (HKCU + política GPO HKLM), bloqueio global de apps UWP em 2º plano (LetAppsRunInBackground=2), CrossDevice Resume, amostragem de energia SRUM (EnergyEstimationEnabled=0) e atraso de logon.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance", "MaintenanceDisabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy", "01", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\StorageSense", "AllowStorageSenseGlobal", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\StorageSense", "AllowStorageSenseTemporaryFilesCleanup", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsRunInBackground", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CrossDeviceResume\Configuration", "IsResumeAllowed", dw(0)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "EnergyEstimationEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "StartupDelayInMSec", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableFirstLogonAnimation", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance", "MaintenanceDisabled"),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy", "01", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\StorageSense", "AllowStorageSenseGlobal"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\StorageSense", "AllowStorageSenseTemporaryFilesCleanup"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsRunInBackground"),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\CrossDeviceResume\Configuration", "IsResumeAllowed", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\Power", "EnergyEstimationEnabled", dw(1)),
            RegOp::delete(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "StartupDelayInMSec"),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableFirstLogonAnimation", dw(1)),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.ucpd_sppsvc",
            name: "Desativar filtro UCPD (24H2) e colocar sppsvc em Manual",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Grava via REG_OPTION_BACKUP_RESTORE Start=4 no minifiltro UCPD (que trava políticas no 24H2) e Start=3/DelayedAutostart=0 no sppsvc mantendo a ativação do Windows.",
            details: vec![],
        },
        vec![
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\UCPD", "Start", dw(4)),
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\sppsvc", "Start", dw(3)),
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\sppsvc", "DelayedAutostart", dw(0)),
        ],
        vec![
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\UCPD", "Start", dw(1)),
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\sppsvc", "Start", dw(2)),
            RegOp::set_br(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\sppsvc", "DelayedAutostart", dw(1)),
        ],
    )));

    list.push(Box::new(ActionCenterSsoTweak::new()));
    list.push(Box::new(OptionalFeaturesTweak::new()));
    list.push(Box::new(SystemAppxTweak::new()));
    list.push(Box::new(ConsumerAppxTweak::new()));
    list.push(Box::new(FirewallOrphansTweak::new()));
    list.push(Box::new(PnpUnusedDevicesTweak::new()));

    // 7. SERVIÇOS DO WINDOWS (granulares e sem duplicatas)
    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.windows_update_telemetry",
            name: "Serviços: Windows Update, Telemetria, Diagnóstico e BITS",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa wuauserv, UsoSvc, WaaSMedicSvc (BACKUP_RESTORE), DoSvc, DiagTrack, dcsvc, dmwappushservice, whesvc, DPS, WdiServiceHost, WdiSystemHost, WerSvc, wercplsupport, TroubleshootingSvc, diagsvc e BITS.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable("wuauserv"),
            ServiceTarget::disable("UsoSvc"),
            ServiceTarget { use_backup_restore: true, ..ServiceTarget::disable("WaaSMedicSvc") },
            ServiceTarget::disable_default_auto("DoSvc"),
            ServiceTarget::disable_default_auto("DiagTrack"),
            ServiceTarget::disable("dcsvc"),
            ServiceTarget::disable("dmwappushservice"),
            ServiceTarget::disable("whesvc"),
            ServiceTarget::disable_default_auto("DPS"),
            ServiceTarget::disable("WdiServiceHost"),
            ServiceTarget::disable("WdiSystemHost"),
            ServiceTarget::disable("WerSvc"),
            ServiceTarget::disable("wercplsupport"),
            ServiceTarget::disable("TroubleshootingSvc"),
            ServiceTarget::disable("diagsvc"),
            ServiceTarget { clear_delayed: true, ..ServiceTarget::disable_default_auto("BITS") },
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.search_print_fax",
            name: "Serviços: Indexação (WSearch), Impressão, Scanner, Mapas e Autoplay",
            category: Category::WindowsServices,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa indexação do Windows Search, spooler de impressão/scanner, mapas offline e reprodução automática. Atenção: impressoras, scanners e busca indexada param de funcionar até reverter.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("WSearch"),
            ServiceTarget::disable_default_auto("Spooler"),
            ServiceTarget::disable("PrintDeviceConfigurationService"),
            ServiceTarget::disable("PrintScanBrokerService"),
            ServiceTarget { use_backup_restore: true, ..ServiceTarget::disable("PrintWorkflowUserSvc") },
            ServiceTarget::disable_default_auto("MapsBroker"),
            ServiceTarget::disable("StiSvc"),
            ServiceTarget::disable("WiaRpc"),
            ServiceTarget::disable_default_auto("ShellHWDetection"),
            ServiceTarget::disable("NPSMSvc"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.eventlog_off",
            name: "Serviço: Desativar EventLog (Log de Eventos do Windows)",
            category: Category::WindowsServices,
            risk: RiskLevel::High,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o Windows Event Log, reduzindo escritas contínuas em disco e overhead em segundo plano. Atenção: impede visualizar registros de erros e eventos no Visualizador de Eventos.",
            details: vec![],
        },
        vec![ServiceTarget::disable_default_auto("EventLog")],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.hidserv_off",
            name: "Serviço: Desativar hidserv (Botões de Volume/Mídia do teclado)",
            category: Category::WindowsServices,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o serviço de acesso a dispositivos de interface humana (teclas de volume/mídia param; teclado e mouse continuam normais).",
            details: vec![],
        },
        vec![ServiceTarget::disable("hidserv")],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.gameinput_off",
            name: "Servico: Desativar GameInputSvc (Microsoft GameInput)",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o servico Microsoft GameInput (GameInputSvc) para quem nao usa jogos/controles baseados em GameInput. Ha relatos de usuarios ligando-o a picos de DPC no 24H2, mas nenhuma medicao publicada confirma; nao espere ganho de FPS. Atencao: controles Xbox ou joysticks em jogos que usam GameInput podem parar ate reverter; teclado e mouse continuam normais.",
            details: vec![],
        },
        vec![ServiceTarget::disable("GameInputSvc")],
    )));

    // Mesma decisao da ISO (Perfil_01LowOS.ps1 / Aplicar_PosOOBE.ps1, testado no PC em 20/10/2026).
    // Servicos ausentes (ex.: Defender ja removido) ficam N/A pelo proprio ServiceGroupTweak.
    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.gamedvr_webthreat_off",
            name: "Servicos: Transmissao GameDVR (BcastDVRUserService) e Web Threat Defense",
            category: Category::WindowsServices,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o servico por usuario de captura/transmissao do GameDVR (sem uso com GameDVR desligado em gpu.game_mode) e os servicos Web Threat Defense da Protecao Avancada contra Phishing (sem uso com SmartScreen desligado).",
            details: vec![],
        },
        vec![
            ServiceTarget::disable("BcastDVRUserService"),
            ServiceTarget::disable("webthreatdefsvc"),
            ServiceTarget::disable("webthreatdefusersvc"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.smb_network_off",
            name: "Serviços de Rede: SMB (LanmanWorkstation e LanmanServer)",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa cliente/servidor SMB de pastas compartilhadas na rede local. iphlpsvc, Wcmsvc e WinHttpAutoProxySvc são preservados para manter ExitLag, NoPing, GearUP e VPNs funcionando. Atenção: compartilhamento de arquivos/impressoras na rede local deixa de funcionar até reverter.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("LanmanWorkstation"),
            ServiceTarget::disable_default_auto("LanmanServer"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.wifi_wlan_off",
            name: "Serviços de Wi-Fi: Desativar WlanSvc, Wi-Fi Direct e configuração sem fio",
            category: Category::WindowsServices,
            risk: RiskLevel::High,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa a pilha de rede sem fio (WlanSvc, wlpasvc, wcncsvc e Wi-Fi Direct), eliminando varreduras periódicas de SSID em segundo plano. Atenção: aplique SOMENTE se usar cabo Ethernet — o Wi-Fi para de funcionar até reverter.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("WlanSvc"),
            ServiceTarget::disable("wlpasvc"),
            ServiceTarget::disable("wcncsvc"),
            ServiceTarget::disable("WFDSConMgrSvc"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.themes_storsvc_sysmain_off",
            name: "Serviços de Sistema: Themes, StorSvc, SysMain, DusmSvc e Ndu",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa Temas, Servico de Armazenamento, Superfetch/SysMain, Uso de Dados (DusmSvc) e driver de contagem de dados (Ndu). Atencao: com jogos em HD mecanico o SysMain acelera o carregamento; nesse caso nao aplique.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("Themes"),
            ServiceTarget::disable("StorSvc"),
            ServiceTarget::disable_default_auto("SysMain"),
            ServiceTarget::disable_default_auto("DusmSvc"),
            ServiceTarget::disable("Ndu"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.sens_eventsystem_off",
            name: "Serviços COM+: SENS, EventSystem e COMSysApp",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa o Sistema de Eventos COM+ e Notificação de Eventos do Sistema em segundo plano, reduzindo processos e threads ociosos.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("SENS"),
            ServiceTarget::disable_default_auto("EventSystem"),
            ServiceTarget::disable("COMSysApp"),
        ],
    )));

    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.drivers_legacy",
            name: "Drivers de Kernel sem uso (wtd, kdnic, rdpbus, CSC/SMB, NetBIOS, LLTD/LLDP)",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Define Start=4 em drivers sem uso (wtd, kdnic, rdpbus, CSC, Dfsc, NetBT, NetBIOS, Beep, rdbss, lltdio, rspndr, MsLldp). Preserva filtros NDIS, CldFlt (OneDrive/Arquivos na Nuvem) e mantém Vid/storqosflt se Hyper-V estiver instalado.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable("wtd"),
            ServiceTarget::disable("kdnic"),
            ServiceTarget::disable("rdpbus"),
            ServiceTarget::disable("CSC"),
            ServiceTarget::disable("Dfsc"),
            ServiceTarget::disable("NetBT"),
            ServiceTarget::disable("NetBIOS"),
            ServiceTarget::disable("Beep"),
            ServiceTarget::disable("rdbss"),
            ServiceTarget::disable("lltdio"),
            ServiceTarget::disable("rspndr"),
            ServiceTarget::disable("MsLldp"),
            ServiceTarget { skip_if_hyperv: true, ..ServiceTarget::disable("Vid") },
            ServiceTarget { skip_if_hyperv: true, ..ServiceTarget::disable("storqosflt") },
        ],
    )));

    let misc_disable = [
        "AarSvc", "AJRouter", "ALG", "AppIDSvc", "AppMgmt", "AppVClient", "AssignedAccessManagerSvc",
        "autotimesvc", "AxInstSV", "BDESVC", "BTAGService", "BthAvctpSvc", "CDPSvc", "CDPUserSvc",
        "CertPropSvc", "CloudBackupRestoreSvc", "CscService", "DisplayEnhancementService",
        "DmEnrollmentSvc", "dot3svc", "DsSvc", "EapHost", "EFS", "embeddedmode", "EntAppSvc",
        "fdPHost", "FDResPub", "fhsvc", "FrameServer", "FrameServerMonitor", "GraphicsPerfSvc",
        "HvHost", "icssvc", "InventorySvc", "IpxlatCfgSvc", "KtmRm", "lfsvc", "lltdsvc", "lmhosts",
        "LocalKdc", "LxpSvc", "MixedRealityLinkSvc", "MSiSCSI", "NaturalAuthentication", "NcaSvc",
        "NcdAutoSetup", "Netlogon", "NetTcpPortSharing", "PcaSvc", "PeerDistSvc", "perceptionsimulation",
        "PhoneSvc", "PimIndexMaintenanceSvc", "pla", "PushToInstall", "RasAuto", "RemoteAccess",
        "RemoteRegistry", "RetailDemo", "RpcLocator", "SCardSvr", "ScDeviceEnum", "SCPolicySvc",
        "SDRSVC", "seclogon", "SEMgrSvc", "SensorDataService", "SensrSvc", "SessionEnv", "SharedAccess",
        "shpamsvc", "smphost", "SmsRouter", "SNMPTrap", "SstpSvc", "svsvc", "TapiSrv", "TermService",
        "TieringEngineService", "TrkWks", "tzautoupdate", "UevAgentService", "UmRdpService",
        "UnistoreSvc", "upnphost", "UserDataSvc", "vmicguestinterface", "vmicheartbeat", "vmickvpexchange",
        "vmicrdv", "vmicshutdown", "vmictimesync", "vmicvmsession", "vmicvss", "WalletService",
        "WebClient", "Wecsvc", "WEPHOSTSVC", "WinRM", "wisvc",
        "wmiApSrv", "WMPNetworkSvc", "workfolderssvc", "WpcMonSvc", "OneSyncSvc", "PolicyAgent", "QWAVE",
        // Pos-OOBE extras (sem PrintDeviceConfigurationService que pertence a svc.search_print_fax)
        "WSAIFabricSvc", "McpManagementService", "ADPSvc", "wuqisvc",
        "McmSvc", "WwanSvc", "MessagingService", "hpatchmon", "refsdedupsvc", "ZTHELPER", "P9RdrService",
        "PenService", "IsolationSession", "IsoSessionCore", "ApxSvc", "midisrv",
        "WPDBusEnum", "NcbService", "WManSvc",
        "IKEEXT", "MSDTC", "wbengine", "PerfHost", "VSS", "swprv", "XboxGipSvc", "RasMan", "SSDPSRV",
    ];
    let mut misc_targets: Vec<ServiceTarget> = misc_disable
        .iter()
        .map(|&n| ServiceTarget::disable(n))
        .collect();
    misc_targets.push(ServiceTarget::manual("AppXSvc"));
    list.push(Box::new(ServiceGroupTweak::new(
        TweakMeta {
            id: "svc.misc_non_essential",
            name: "Serviços secundários sem uso para jogos (Fax/RDP/Hyper-V guest/XboxGip/etc.)",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa serviços secundários de servidor, RDP, cartões inteligentes, celular, backup e áudio MIDI, e ajusta AppXSvc para Manual. Preserva Conta Microsoft, Windows Hello/PIN, Wi-Fi, Bluetooth, áudio, notificações e loja.",
            details: vec![],
        },
        misc_targets,
    )));

    // 8. TERCEIROS
    list.push(Box::new(ThirdPartyBloatTweak::new()));

    // 9. TAREFAS AGENDADAS
    let win_task_patterns = [
        r"\Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser",
        r"\Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser Exp",
        r"\Microsoft\Windows\Application Experience\MareBackup",
        r"\Microsoft\Windows\Application Experience\StartupAppTask",
        r"\Microsoft\Windows\Customer Experience Improvement Program\Consolidator",
        r"\Microsoft\Windows\Customer Experience Improvement Program\UsbCeip",
        r"\Microsoft\Windows\Autochk\Proxy",
        r"\Microsoft\Windows\DiskDiagnostic\Microsoft-Windows-DiskDiagnosticDataCollector",
        r"\Microsoft\Windows\Feedback\Siuf\DmClient",
        r"\Microsoft\Windows\Feedback\Siuf\DmClientOnScenarioDownload",
        r"\Microsoft\Windows\Windows Error Reporting\QueueReporting",
        r"\Microsoft\Windows\Maintenance\WinSAT",
        r"\Microsoft\Windows\Power Efficiency Diagnostics\AnalyzeSystem",
        r"\Microsoft\Windows\Diagnosis\Scheduled",
        r"\Microsoft\Windows\Diagnosis\RecommendedTroubleshootingScanner",
        r"\Microsoft\Windows\Diagnosis\UnexpectedCodepath",
        r"\Microsoft\Windows\Shell\FamilySafetyMonitor",
        r"\Microsoft\Windows\Shell\FamilySafetyRefreshTask",
        r"\Microsoft\Windows\Shell\IndexerAutomaticMaintenance",
        r"\Microsoft\Windows\Shell\ThemesSyncedImageDownload",
        r"\Microsoft\Windows\Shell\UpdateUserPictureTask*",
        r"\Microsoft\Windows\Device Information\Device",
        r"\Microsoft\Windows\Device Information\Device User",
        r"\Microsoft\Windows\Maps\*",
        r"\Microsoft\Windows\Location\*",
        r"\Microsoft\Windows\Work Folders\*",
        r"\Microsoft\Windows\Workplace Join\*",
        r"\Microsoft\Windows\Clip\License Validation",
        r"\Microsoft\Windows\Offline Files\*",
        r"\Microsoft\Windows\Printing\*",
        r"\Microsoft\Windows\Mobile Broadband Accounts\*",
        r"\Microsoft\Windows\RetailDemo\*",
        r"\Microsoft\Windows\SpacePort\*",
        r"\Microsoft\Windows\Speech\*",
        r"\Microsoft\Windows\UNP\RunUpdateNotificationMgr",
        r"\Microsoft\Windows\International\Synchronize Language Settings",
        r"\Microsoft\Windows\LanguageComponentsInstaller\*",
        r"\Microsoft\Windows\Management\Provisioning\*",
        r"\Microsoft\Windows\NetTrace\GatherNetworkInfo",
        r"\Microsoft\Windows\input\*",
        r"\Microsoft\Windows\WCM\WiFiTask",
        r"\Microsoft\Windows\PI\Sqm-Tasks",
        r"\Microsoft\Windows\PI\Secure-Boot-Update",
        r"\Microsoft\Windows\SettingSync\*",
        r"\Microsoft\Windows\Device Setup\Metadata Refresh",
        r"\Microsoft\Windows\DiskFootprint\*",
        r"\Microsoft\Windows\Sysmain\*",
        r"\Microsoft\Windows\FileHistory\*",
        r"\Microsoft\Windows\Ras\MobilityManager",
        r"\Microsoft\Windows\Flighting\FeatureConfig\BootstrapUsageDataReporting",
        r"\Microsoft\Windows\Flighting\FeatureConfig\UsageDataReceiver",
        r"\Microsoft\Windows\Flighting\FeatureConfig\UsageDataFlushing",
        r"\Microsoft\Windows\Flighting\FeatureConfig\UsageDataReporting",
        r"\Microsoft\Windows\Flighting\*",
        r"\Microsoft\Windows\AppListBackup\*",
        r"\Microsoft\Windows\CloudRestore\*",
        r"\Microsoft\Windows\Data Integrity Scan\*",
        r"\Microsoft\Windows\DeviceDirectoryClient\*",
        r"\Microsoft\Windows\EDP\*",
        r"\Microsoft\Windows\MemoryDiagnostic\*",
        r"\Microsoft\Windows\PushToInstall\*",
        r"\Microsoft\Windows\RemoteAssistance\*",
        r"\Microsoft\Windows\Sustainability\*",
        r"\Microsoft\Windows\WlanSvc\*",
        r"\Microsoft\Windows\WwanSvc\*",
        r"\Microsoft\Windows\Chkdsk\ProactiveScan",
        r"\Microsoft\Windows\PerformanceTrace\*",
        r"\Microsoft\Windows\NlaSvc\WiFiTask",
        r"\Microsoft\Windows\UpdateOrchestrator\*",
        r"\Microsoft\Windows\WindowsUpdate\*",
        r"\Microsoft\Windows\InstallService\*",
        r"\Microsoft\Windows\Hotpatch\*",
        r"\Microsoft\Windows\WindowsAI\*",
        r"\Microsoft\Windows\Setup\PITRTask",
        r"\Microsoft\Windows\Setup\SetupRecoveryDataTask",
        r"\Microsoft\Windows\Setup\SetupCleanupTask",
        r"\Microsoft\Windows\UsageAndQualityInsights\*",
        r"\Microsoft\Windows\ConsentUX\*",
        r"\Microsoft\Windows\AccountHealth\*",
        r"\Microsoft\Windows\EnterpriseMgmt\*",
        r"\Microsoft\Windows\Active Directory Rights Management Services Client\*",
        r"\Microsoft\Windows\AppID\EDP Policy Manager",
        r"\Microsoft\Windows\ReFsDedupSvc\*",
        r"\Microsoft\Windows\Storage Tiers Management\*",
        r"\Microsoft\Windows\Windows Defender\*",
        r"\Microsoft\Windows\UPnP\UPnPHostConfig",
        r"\Microsoft\Windows\WDI\ResolutionHost",
        r"\Microsoft\Windows\CertificateServicesClient\*",
        r"\Microsoft\Windows\ExploitGuard\*",
        r"\Microsoft\Windows\Network Connectivity Status Indicator\NcsiIdentifyUserProxies",
        r"\Microsoft\Windows\Wininet\CacheTask",
        r"\Microsoft\Windows\Multimedia\SystemSoundsService",
        r"\Microsoft\Windows\DUSM\dusmtask",
        r"\Microsoft\Windows\ApplicationData\*",
        r"\Microsoft\Windows\Containers\CmCleanup",
        r"\Microsoft\Windows\Kernel\La57Cleanup",
        r"\Microsoft\Windows\MUI\LPRemove",
        r"\Microsoft\Windows\Registry\RegIdleBackup",
        r"\Microsoft\Windows\Plug and Play\Sysprep Generalize Drivers",
        r"\Microsoft\Windows\Bluetooth\UninstallDeviceTask",
        r"\Microsoft\Windows\Servicing\StartComponentCleanup",
        r"\Microsoft\Windows\.NET Framework\.NET Framework NGEN v4.0.30319*",
        r"\Microsoft\Windows\BitLocker\*",
        r"\Microsoft\Windows\PCRPF\*",
        r"\Microsoft\Windows\SystemRestore\SR",
        r"\Microsoft\Windows\DirectX\DirectXDatabaseUpdater",
        r"\Microsoft\Windows\DiskCleanup\SilentCleanup",
        r"\Microsoft\Windows\License Manager\TempSignedLicenseExchange",
        r"\Microsoft\Windows\WOF\WIM-Hash-*",
        r"\Microsoft\Windows\AppxDeploymentClient\UCPD velocity",
        r"\Microsoft\XblGameSave\XblGameSaveTask",
        r"\SoftLanding\*",
        r"\Microsoft\Windows\Windows Media Sharing\*",
        r"\Microsoft\Windows\Application Experience\PcaPatchDbTask",
        r"\Microsoft\Windows\Application Experience\SdbinstMergeDbTask",
        r"\Microsoft\Windows\USB\Usb-Notifications",
    ];
    list.push(Box::new(ScheduledTaskGroupTweak::new(
        TweakMeta {
            id: "tasks.windows_bloat",
            name: "Tarefas Agendadas: Desativar telemetria, Update, CEIP, manutenção ociosa e diagnóstico",
            category: Category::ScheduledTasks,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa tarefas agendadas de telemetria, CEIP, UpdateOrchestrator, manutenção em segundo plano (WinSAT, Defrag/Retrim preservado) e uso de dados. Tarefas protegidas por SYSTEM/TrustedInstaller no Windows 11 24H2+ são detectadas via ACL/SDDL e ignoradas sem erro.",
            details: vec![],
        },
        win_task_patterns
            .iter()
            .map(|p| TaskPattern {
                full_like: (*p).to_string(),
                stop_running: false,
                optional: true,
            })
            .collect(),
    )));

    list.push(Box::new(ScheduledTaskGroupTweak::new(
        TweakMeta {
            id: "tasks.legacy_cleanup",
            name: "Tarefas Agendadas Legadas: Desativar tarefas antigas do 01LowOS",
            category: Category::ScheduledTasks,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa tarefas antigas caso existam (ReaplicarAjustesHardware, IsolateAudioDG, LockHighResTimer, DXVK_Global_AutoDeploy).",
            details: vec![],
        },
        [
            r"\ReaplicarAjustesHardware",
            r"\IsolateAudioDG",
            r"\LockHighResTimer",
            r"\DXVK_Global_AutoDeploy",
        ]
        .iter()
        .map(|p| TaskPattern {
            full_like: (*p).to_string(),
            stop_running: true,
            optional: false,
        })
        .collect(),
    )));

    // 10. ETW AUTOLOGGERS
    list.push(Box::new(EtwAutologgerTweak::new()));

    list
}

pub const SAFE_GAMING_PRESET: &[&str] = &[
    "gpu.hags",
    "gpu.mpo_off",
    "gpu.windowed_optimizations",
    "gpu.game_mode",
    "gpu.gamebar_presence_off",
    "gpu.mmcss_games",
    "gpu.presentmon_fps",
    "gpu.msi_mode",
    "gpu.nvidia_telemetry_off",
    "sys.audio_enhancements_off",
    "sys.legacy_cleanup_reg",
    "sys.legacy_cleanup_irq",
    "sys.bcd_clean_timers",
    "sys.msi_mode_usb_audio",
    "sys.nvme_storage_msi",
    "sys.fast_startup_off",
    "sys.memory_compression_off",
    "sys.mmagent_prelaunch_off",
    "sys.vbs_hvci_off",
    "sys.fth_off",
    "sys.crash_rsop_clean",
    "sys.power_throttling_off",
    "sys.win32_priority_separation",
    "sys.ntfs_low_overhead",
    "sys.ntfs_memory_usage",
    "sys.storage_trim_on",
    "sys.bitlocker_auto_encrypt_off",
    "sys.wpbt_off",
    "sys.prefetcher_off",
    "sys.defender_game_exclusions",
    "sys.defender_scan_throttle",
    "sys.ifeo_touch_voice_off",
    "power.plan_01lowos",
    "power.overlay_best_perf",
    "power.hiberfil_off",
    "power.device_wake_off",
    "power.usb_device_pm_off",
    "power.usb_hub_pm_off",
    "power.usb_d3_suspend_off",
    "power.hdaudio_idle_off",
    "net.wol_eee_off",
    "net.nic_power_mgmt_off",
    "net.llmnr_mdns_wpad_off",
    "net.wininet_wpad_off",
    "net.rsc_udp_latency",
    "net.qos_nic_flow_off",
    "net.netbios_off",
    "user.mouse_raw",
    "user.keyboard_accessibility",
    "user.visual_perf",
    "user.explorer_clean",
    "user.explorer_gallery_quickaccess_clean",
    "user.audio_no_ducking",
    "user.privacy_hkcu",
    "user.nearby_speech_siuf_off",
    "bloat.ai_search_cloud",
    "bloat.store_autoupdate_off",
    "bloat.appcompat_sqm_telemetry",
    "bloat.edge_background_off",
    "bloat.browsers_bg_telemetry_off",
    "bloat.autorun_off",
    "bloat.cli_notepad_ai_telemetry_off",
    "bloat.typing_data_off",
    "bloat.office_telemetry_off",
    "bloat.lockscreen_logon_clean",
    "bloat.maintenance_background_off",
    "bloat.actioncenter_sso",
    "tasks.windows_bloat",
    "tasks.legacy_cleanup",
];

pub fn validate_catalog(catalog: &[Box<dyn Tweak>]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for t in catalog {
        if !ids.insert(t.meta().id) {
            return Err(format!("ID duplicado no catálogo: {}", t.meta().id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::registry_tweak::RegTarget;

    #[test]
    fn catalog_ids_are_unique_and_non_empty() {
        let cat = build_catalog();
        assert!(cat.len() >= 111);
        validate_catalog(&cat).unwrap();
    }

    #[test]
    fn catalog_has_no_conflicting_registry_keys() {
        let cat = build_catalog();
        let mut seen: std::collections::HashMap<String, &'static str> =
            std::collections::HashMap::new();
        for t in &cat {
            let meta = t.meta();
            for detail in &meta.details {
                if (detail.starts_with("HKLM\\") || detail.starts_with("HKCU\\"))
                    && detail.contains(" = ")
                {
                    if let Some((lhs, _)) = detail.split_once(" = ") {
                        let key = lhs.trim().to_ascii_lowercase();
                        if let Some(prev_id) = seen.insert(key.clone(), meta.id) {
                            panic!(
                                "Chave de registro duplicada '{key}' em '{}' e '{}'",
                                prev_id, meta.id
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn catalog_has_no_duplicate_services() {
        let cat = build_catalog();
        let mut seen = HashSet::new();
        for t in &cat {
            let meta = t.meta();
            for d in &meta.details {
                if let Some(rest) = d.strip_prefix("Serviço ") {
                    if let Some(svc_name) = rest.split(':').next() {
                        assert!(
                            seen.insert(svc_name.to_ascii_lowercase()),
                            "Serviço duplicado '{svc_name}' encontrado em {}",
                            meta.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn catalog_respects_claude_md_inviolable_rules() {
        let cat = build_catalog();
        for t in &cat {
            let meta = t.meta();
            assert!(!meta.id.is_empty() && !meta.name.is_empty() && !meta.description.is_empty());
            assert!(!meta.details.is_empty(), "Sem detalhes em {}", meta.id);
            for detail in &meta.details {
                let low = detail.to_ascii_lowercase();
                assert!(!low.contains("disabledynamictick"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("tscsyncpolicy"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("useplatformclock"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("appinit_dlls"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("bioenrollment"), "BioEnrollment quebra Windows Hello! {}", meta.id);
                assert!(!low.contains("disablepagingexecutive"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("largesystemcache"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("enablepreemption"), "Tweak instável proibido em {}", meta.id);
                assert!(!low.contains("rmdisablepostl2compression"), "Tweak instável proibido em {}", meta.id);
                assert!(!low.contains("maximumdpcqueuedepth"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("svchostsplitthresholdinkb"), "SvcHost collapse proibido em {}", meta.id);
                assert!(!low.contains("tcpwindowsize"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("irpstacksize"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("nonpagedpoolsize"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("disabletaskoffload"), "Tweak toxico proibido em {}", meta.id);
                assert!(!low.contains("stuttermode"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("kmd_enableinternallargepage"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("rmgpspsenablepercpucoredpc"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("ecncapability"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("nonsackrttresiliency"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("mousedataqueuesize"), "Tweak instavel proibido em {}", meta.id);
                assert!(!low.contains("keyboarddataqueuesize"), "Tweak instavel proibido em {}", meta.id);
                assert!(!low.contains("disableautodaylighttimeset"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("powermizerenable"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("perflevelsrc"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("powermizerlevel"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("minimumstackcommitinbytes"), "Tweak perigoso proibido em {}", meta.id);
                assert!(!low.contains("splitlargecaches"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("latencyhintperf"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("latencyhintunpark"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("intsteermode"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("scanwhenassociated"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("checksumoffload"), "Tweak toxico proibido em {}", meta.id);
                assert!(!low.contains("coalescingtimerinterval"), "Placebo proibido em {}", meta.id);
                assert!(!low.contains("disabledmacopy"), "Placebo proibido em {}", meta.id);
                if !low.starts_with("remover ") {
                    assert!(!low.contains("iopagelocklimit"), "Placebo proibido em {}", meta.id);
                    assert!(!low.contains("globaltimerresolutionrequests"), "Timer global proibido em {}", meta.id);
                    assert!(!low.contains("serializetimerexpiration"), "Placebo proibido em {}", meta.id);
                }
                assert!(!low.contains("wpnservice: disabled"), "WpnService deve ficar mantido! {}", meta.id);
                assert!(!low.contains("wpnuserservice: disabled"), "WpnUserService deve ficar mantido! {}", meta.id);
                assert!(!low.contains("camsvc: disabled"), "camsvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("cbdhsvc: disabled"), "cbdhsvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("sensorservice: disabled"), "SensorService deve ficar mantido! {}", meta.id);
                assert!(!low.contains("licensemanager: disabled"), "LicenseManager deve ficar mantido! {}", meta.id);
                assert!(!low.contains("installservice: disabled"), "InstallService deve ficar mantido! {}", meta.id);
                assert!(!low.contains("noconnecteduser"), "NoConnectedUser quebra Conta Microsoft! {}", meta.id);
                assert!(!low.contains("passportforwork"), "PassportForWork quebra PIN/Hello! {}", meta.id);
                assert!(!low.contains("tokenbroker: disabled"), "TokenBroker deve ficar mantido! {}", meta.id);
                assert!(!low.contains("wlidsvc: disabled"), "wlidsvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("ngcsvc: disabled"), "NgcSvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("ngcctnrsvc: disabled"), "NgcCtnrSvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("wbiosrvc: disabled"), "WbioSrvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("credentialenrollmentmanagerusersvc: disabled"), "CredentialEnrollmentManagerUserSvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("cloudidsvc: disabled"), "cloudidsvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("usermanager: disabled"), "UserManager deve ficar mantido! {}", meta.id);
                assert!(!low.contains("profsvc: disabled"), "ProfSvc deve ficar mantido! {}", meta.id);
                assert!(!low.contains("cldflt: disabled"), "CldFlt quebra arquivos/atalhos do OneDrive! {}", meta.id);
                assert!(!low.contains("iphlpsvc: disabled"), "iphlpsvc deve ficar mantido para ExitLag/VPN! {}", meta.id);
                assert!(!low.contains("wcmsvc: disabled"), "Wcmsvc deve ficar mantido para ExitLag/VPN! {}", meta.id);
                assert!(!low.contains("ndisvirtualbus"), "NdisVirtualBus deve ficar mantido para ExitLag/VPN! {}", meta.id);
                assert!(!low.contains("winhttpautoproxysvc: disabled"), "WinHttpAutoProxySvc é dependência de iphlpsvc/Wcmsvc! {}", meta.id);
            }
        }

        let legacy = cat
            .iter()
            .find(|t| t.meta().id == "sys.legacy_cleanup_reg")
            .unwrap();
        assert!(matches!(RegTarget::Delete, RegTarget::Delete));
        assert!(legacy
            .meta()
            .details
            .iter()
            .any(|d| d.starts_with("REMOVER ") && d.contains("GlobalTimerResolutionRequests")));

        let hags = cat.iter().find(|t| t.meta().id == "gpu.hags").unwrap();
        assert!(hags.can_reset_default());
    }

    #[test]
    fn safe_gaming_preset_ids_exist_in_catalog() {
        let cat = build_catalog();
        let ids: HashSet<&str> = cat.iter().map(|t| t.meta().id).collect();
        for &preset_id in SAFE_GAMING_PRESET {
            assert!(
                ids.contains(preset_id),
                "ID do Preset Jogos Seguro ausente no catálogo: {preset_id}"
            );
            let tw = cat.iter().find(|t| t.meta().id == preset_id).unwrap();
            assert_ne!(
                tw.meta().risk,
                RiskLevel::High,
                "Tweak de risco Alto não pode estar no Preset Seguro: {preset_id}"
            );
        }
    }
}
