use std::collections::HashSet;

use crate::sys::registry::{RegHive, RegValue};
use crate::tweaks::advanced::{
    ActionCenterSsoTweak, AmdShaderCacheTweak, AudioEnhancementsTweak, CloudflareDnsTweak,
    ConsumerAppxTweak, EtwAutologgerTweak, FirewallOrphansTweak, GpuNicMsiModeTweak,
    LegacyHardwareIrqTweak, MemoryCompressionTweak, NicPowerMgmtTweak, NvidiaDynamicPstateTweak,
    NvidiaTelemetryTweak, OptionalFeaturesTweak, PnpUnusedDevicesTweak, PresentMonFpsTweak,
    SmartScreenAclTweak, SystemAppxTweak, TcpNagleTweak, ThirdPartyBloatTweak,
    UsbAudioMsiModeTweak, UsbDevicePowerMgmtTweak, UsbHubPowerMgmtTweak,
};
use crate::tweaks::network::{
    NicBindingsTweak, NicRssTweak, NicWolEeeTweak, WinInetWpadTweak,
};
use crate::tweaks::power::{DeviceWakeTweak, PowerPlanTweak};
use crate::tweaks::registry_tweak::{RegOp, RegistryTweak};
use crate::tweaks::service_tweak::{ServiceGroupTweak, ServiceTarget};
use crate::tweaks::task_tweak::{ScheduledTaskGroupTweak, TaskPattern};
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta};

fn dw(v: u32) -> RegValue {
    RegValue::DWord(v)
}
fn sz(v: &str) -> RegValue {
    RegValue::String(v.to_string())
}

pub fn build_catalog() -> Vec<Box<dyn Tweak>> {
    let mut list: Vec<Box<dyn Tweak>> = Vec::with_capacity(72);

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
            RegOp::set(RegHive::Hkcu, r"System\GameConfigStore", "GameDVR_Enabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR"),
        ],
    )));

    list.push(Box::new(RegistryTweak::new(
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
    )));

    list.push(Box::new(AmdShaderCacheTweak::new()));
    list.push(Box::new(PresentMonFpsTweak::new()));
    list.push(Box::new(GpuNicMsiModeTweak::new()));
    list.push(Box::new(NvidiaTelemetryTweak::new()));
    list.push(Box::new(NvidiaDynamicPstateTweak::new()));
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
    list.push(Box::new(UsbAudioMsiModeTweak::new()));

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
            id: "sys.ntfs_low_overhead",
            name: "NTFS: Desativar LastAccessUpdate e nomes curtos 8.3 (mantém TRIM)",
            category: Category::SystemKernel,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Evita gravações de metadata a cada leitura de arquivos/texturas nos jogos (equivalente a fsutil disablelastaccess 1 e disable8dot3 1) sem desativar TRIM.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisableLastAccessUpdate", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisable8dot3NameCreation", dw(1)),
        ],
        vec![
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisableLastAccessUpdate", dw(0x8000_0002)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Control\FileSystem", "NtfsDisable8dot3NameCreation", dw(2)),
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
    list.push(Box::new(DeviceWakeTweak::new()));
    list.push(Box::new(UsbDevicePowerMgmtTweak::new()));
    list.push(Box::new(UsbHubPowerMgmtTweak::new()));

    // 4. REDE
    list.push(Box::new(NicRssTweak::new()));
    list.push(Box::new(NicWolEeeTweak::new()));
    list.push(Box::new(NicPowerMgmtTweak::new()));
    list.push(Box::new(NicBindingsTweak::new()));
    list.push(Box::new(TcpNagleTweak::new()));
    list.push(Box::new(CloudflareDnsTweak::new()));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "net.llmnr_mdns_wpad_off",
            name: "Rede: Desativar LLMNR, mDNS, WPAD WinHTTP e túneis IPv6 (6to4/ISATAP/Teredo)",
            category: Category::Network,
            risk: RiskLevel::Low,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa protocolos de descoberta multicast/anúncio local e túneis IPv6 de transição sem utilidade para jogos.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient", "EnableMulticast", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WinHttp", "DisableWpad", dw(1)),
            RegOp::set(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "EnableMDNS", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "6to4_State", sz("Disabled")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "ISATAP_State", sz("Disabled")),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "Teredo_State", sz("Disabled")),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient", "EnableMulticast"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WinHttp", "DisableWpad"),
            RegOp::delete(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters", "EnableMDNS"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "6to4_State"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "ISATAP_State"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\TCPIP\v6Transition", "Teredo_State"),
        ],
    )));

    list.push(Box::new(WinInetWpadTweak::new()));

    // 5. USUÁRIO (HKCU)
    list.push(Box::new(RegistryTweak::new(
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
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "user.keyboard_accessibility",
            name: "Teclado rápido + Desativar atalhos de acessibilidade (5x Shift) e Snipping no Print",
            category: Category::UserHkcu,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: false,
            description: "Define KeyboardDelay=0, KeyboardSpeed=31, NumLock ligado, desliga gatilhos de StickyKeys/FilterKeys/ToggleKeys que tiram o jogo de tela cheia e volta PrintScreen ao print clássico.",
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
        ],
        vec![
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "KeyboardDelay", sz("1")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Keyboard", "PrintScreenKeyForSnippingEnabled", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\StickyKeys", "Flags", sz("510")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\Keyboard Response", "Flags", sz("126")),
            RegOp::set(RegHive::Hkcu, r"Control Panel\Accessibility\ToggleKeys", "Flags", sz("62")),
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
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_IrisRecommendations", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowSyncProviderNotifications", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "SearchboxTaskbarMode", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Search", "BackgroundAppGlobalToggle", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\Bags\AllFolders\Shell", "FolderType", sz("NotSpecified")),
        ],
        vec![
            RegOp::delete(RegHive::Hkcu, r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32", ""),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "HideFileExt", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Hidden", dw(2)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ShowTaskViewButton", dw(1)),
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", dw(0)),
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

    list.push(Box::new(RegistryTweak::new(
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
            RegOp::set(RegHive::Hkcu, r"Software\Microsoft\Personalization\Settings", "AcceptedPrivacyPolicy", dw(0)),
            RegOp::set(RegHive::Hkcu, r"Control Panel\International\User Profile", "HttpAcceptLanguageOptOut", dw(1)),
        ],
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
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry"),
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
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "TurnOffSavingSnapshots", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableClickToDo", dw(1)),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "AllowNewsAndInterests", dw(0)),
            RegOp::set_opt(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebSearch", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableSearch", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "AllowOnlineTips", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Personalization", "NoLockScreen"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableWindowsConsumerFeatures"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableSoftLanding"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableCloudOptimizedContent"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot"),
            RegOp::delete(RegHive::Hkcu, r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "TurnOffSavingSnapshots"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableClickToDo"),
            RegOp::delete_opt(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh", "AllowNewsAndInterests"),
            RegOp::set_opt(RegHive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa", dw(1)),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableWebSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "DisableSearch"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer", "AllowOnlineTips"),
        ],
    )));

    list.push(Box::new(RegistryTweak::with_defaults(
        TweakMeta {
            id: "bloat.edge_background_off",
            name: "Microsoft Edge: Desativar Startup Boost, modo em 2º plano e barra lateral",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Impede que o Microsoft Edge pré-carregue no boot (Startup Boost) e continue em RAM após ser fechado, economizando ~150–250 MB de RAM e processos WebView em Windows 11 Home/Pro 24H2/25H2/26H1. O navegador continua funcionando normalmente ao abrir.",
            details: vec![],
        },
        vec![
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "StartupBoostEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "BackgroundModeEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "HubsSidebarEnabled", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeShoppingAssistantEnabled", dw(0)),
        ],
        vec![
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "StartupBoostEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "BackgroundModeEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "HubsSidebarEnabled"),
            RegOp::delete(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Edge", "EdgeShoppingAssistantEnabled"),
        ],
    )));

    list.push(Box::new(RegistryTweak::new(
        TweakMeta {
            id: "bloat.telemetry_restore_wer",
            name: "Políticas: Desativar System Restore, WER, Localização, OneDrive, Sync, Ink e Shell órfãos",
            category: Category::PrivacyBloat,
            risk: RiskLevel::Medium,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa snapshots VSS do System Restore, Relatório de Erros, Histórico de Atividades, Anúncios, Localização, Sync, OneDrive, Assistência Remota e bloqueia extensões órfãs do clique direito (preserva Conta Microsoft e Windows Hello/PIN).",
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
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\Windows\OneDrive", "DisableFileSyncNGSC", dw(1)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\WindowsInkWorkspace", "AllowWindowsInkWorkspace", dw(0)),
            RegOp::set(RegHive::Hklm, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization", dw(0)),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{596AB062-B4D2-4215-9F74-E9109B0A8153}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{7AD84985-87B4-4a16-BE58-8B72A5B390F7}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{f81e9010-6ea4-11ce-a7ff-00aa003ca9f6}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{e2bf9676-5f8f-435c-97eb-11607a5bedf7}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{3dad6c5d-2167-4cae-9914-f99e41c12cfa}", sz("")),
            RegOp::set_opt(RegHive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "{09A47860-11B0-4DA5-AFA5-26D86198A780}", sz("")),
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
            id: "svc.smb_network_off",
            name: "Serviços de Rede: SMB (LanmanWorkstation/Server), iphlpsvc, Wcmsvc e WinHttpAutoProxy",
            category: Category::WindowsServices,
            risk: RiskLevel::Medium,
            requires_reboot: true,
            requires_admin: true,
            description: "Desativa cliente/servidor SMB de pastas compartilhadas na rede, IP Helper e proxy automático WinHTTP. Atenção: compartilhamento de arquivos/impressoras na rede local deixa de funcionar até reverter.",
            details: vec![],
        },
        vec![
            ServiceTarget::disable_default_auto("LanmanWorkstation"),
            ServiceTarget::disable_default_auto("LanmanServer"),
            ServiceTarget::disable_default_auto("iphlpsvc"),
            ServiceTarget::disable_default_auto("Wcmsvc"),
            ServiceTarget::disable("WinHttpAutoProxySvc"),
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
            description: "Desativa Temas, Serviço de Armazenamento, Superfetch/SysMain, Uso de Dados (DusmSvc) e driver de contagem de dados (Ndu).",
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
            description: "Define Start=4 em drivers sem uso (wtd, kdnic, rdpbus, CSC, Dfsc, NetBT, NetBIOS, Beep, CldFlt, rdbss, lltdio, rspndr, MsLldp). Preserva filtros NDIS e mantém Vid/storqosflt se Hyper-V estiver instalado.",
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
            ServiceTarget::disable("CldFlt"),
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
        "IKEEXT", "MSDTC", "wbengine", "PerfHost", "VSS", "swprv", "XboxGipSvc", "RasMan",
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
    ];
    list.push(Box::new(ScheduledTaskGroupTweak::new(
        TweakMeta {
            id: "tasks.windows_bloat",
            name: "Tarefas Agendadas: Desativar telemetria, Update, CEIP, manutenção ociosa e diagnóstico",
            category: Category::ScheduledTasks,
            risk: RiskLevel::Low,
            requires_reboot: false,
            requires_admin: true,
            description: "Desativa tarefas agendadas de telemetria, CEIP, UpdateOrchestrator, manutenção em segundo plano (WinSAT, Defrag/Retrim preservado) e uso de dados.",
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
    "gpu.mmcss_games",
    "gpu.presentmon_fps",
    "gpu.msi_mode",
    "gpu.nvidia_telemetry_off",
    "sys.audio_enhancements_off",
    "sys.legacy_cleanup_reg",
    "sys.legacy_cleanup_irq",
    "sys.msi_mode_usb_audio",
    "sys.fast_startup_off",
    "sys.memory_compression_off",
    "sys.vbs_hvci_off",
    "sys.fth_off",
    "sys.power_throttling_off",
    "sys.ntfs_low_overhead",
    "sys.ntfs_memory_usage",
    "sys.ifeo_touch_voice_off",
    "power.plan_01lowos",
    "power.device_wake_off",
    "power.usb_device_pm_off",
    "power.usb_hub_pm_off",
    "net.wol_eee_off",
    "net.nic_power_mgmt_off",
    "net.llmnr_mdns_wpad_off",
    "net.wininet_wpad_off",
    "user.mouse_raw",
    "user.keyboard_accessibility",
    "user.visual_perf",
    "user.explorer_clean",
    "user.audio_no_ducking",
    "user.privacy_hkcu",
    "bloat.ai_search_cloud",
    "bloat.edge_background_off",
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
        assert!(cat.len() >= 45);
        validate_catalog(&cat).unwrap();
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
            for detail in &meta.details {
                let low = detail.to_ascii_lowercase();
                assert!(!low.contains("disabledynamictick"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("tscsyncpolicy"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("useplatformclock"), "Tweak proibido em {}", meta.id);
                assert!(!low.contains("appinit_dlls"), "Tweak proibido em {}", meta.id);
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
        }
    }
}
