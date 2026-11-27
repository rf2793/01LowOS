//! Perfil global do driver NVIDIA (DRS) via NVAPI, carregado dinamicamente de System32.
//! IDs de funcao (nvapi_interface.h), IDs/valores de setting (NvApiDriverSettings.h) e
//! layout de NVDRS_SETTING_V1 (nvapi.h, pack(4)) conferidos no SDK oficial github.com/NVIDIA/nvapi.
//! Recomendacoes do guia PC-Tuning (docs/configure-nvidia.md). Nada e injetado em processo de jogo:
//! so o 01LowOS fala com o driver, que grava o perfil em ProgramData\NVIDIA Corporation\Drs.

use std::ffi::c_void;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

use crate::sys::backup::BackupStore;
use crate::sys::elevation::to_wide_null;
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta, TweakStatus};

// nvapi_interface.h
const ID_INITIALIZE: u32 = 0x0150_e828;
const ID_DRS_CREATE_SESSION: u32 = 0x0694_d52e;
const ID_DRS_DESTROY_SESSION: u32 = 0xdad9_cff8;
const ID_DRS_LOAD_SETTINGS: u32 = 0x375d_bd6b;
const ID_DRS_SAVE_SETTINGS: u32 = 0xfcbc_7e14;
const ID_DRS_GET_CURRENT_GLOBAL_PROFILE: u32 = 0x617b_ff9f;
const ID_DRS_GET_SETTING: u32 = 0x73bf_8338;
const ID_DRS_SET_SETTING: u32 = 0x577d_d202;
const ID_DRS_RESTORE_PROFILE_DEFAULT_SETTING: u32 = 0x53f0_381e;

// nvapi_lite_common.h
const NVAPI_OK: i32 = 0;
const NVAPI_SETTING_NOT_FOUND: i32 = -160;

// nvapi.h: NVDRS_SETTING_TYPE
const NVDRS_DWORD_TYPE: u32 = 0;

// NvApiDriverSettings.h
pub const PRERENDERLIMIT_ID: u32 = 0x007B_A09E; // "Maximum pre-rendered frames" (Low Latency Mode)
pub const PREFERRED_PSTATE_ID: u32 = 0x1057_EB71; // "Power management mode"
pub const PS_SHADERDISKCACHE_MAX_SIZE_ID: u32 = 0x00AC_8497; // "Shader disk cache maximum size"
// ANSEL_ENABLE_ID (0x1075D972) ficou de fora: o driver 621 (RTX 5080) responde NVAPI_NOT_SUPPORTED (-104);
// o Ansel foi aposentado e substituido pelo modo foto do NVIDIA App.

/// (id, valor alvo, rotulo). Low Latency "On" = 1 quadro pre-renderizado; PREFER_MAX = 1;
/// tamanho maximo do cache = PS_SHADERDISKCACHE_MAX_SIZE_MAX (0xffffffff, "Unlimited");
/// (Ansel removido: ver comentario acima.)
pub const TARGETS: &[(u32, u32, &str)] = &[
    (PRERENDERLIMIT_ID, 1, "Low Latency Mode = On (1 quadro pre-renderizado)"),
    (PREFERRED_PSTATE_ID, 1, "Power management mode = Prefer maximum performance"),
    (PS_SHADERDISKCACHE_MAX_SIZE_ID, 0xFFFF_FFFF, "Shader Cache Size = Unlimited"),
];

const UNICODE_STRING_MAX: usize = 2048;
const UNION_WORDS: usize = 4100 / 4; // max(NVDRS_BINARY_SETTING = 4 + 4096, wstring 4096, u64)

/// Espelho de NVDRS_SETTING_V1 (#pragma pack(4)). As unions viram blocos de 4100 bytes; o valor
/// DWORD (u32PredefinedValue / u32CurrentValue) e a primeira palavra de cada uma.
#[repr(C)]
struct NvdrsSetting {
    version: u32,
    setting_name: [u16; UNICODE_STRING_MAX],
    setting_id: u32,
    setting_type: u32,
    setting_location: u32,
    is_current_predefined: u32,
    is_predefined_valid: u32,
    predefined: [u32; UNION_WORDS],
    current: [u32; UNION_WORDS],
}

const NVDRS_SETTING_SIZE: usize = 12320;
const NVDRS_SETTING_VER: u32 = NVDRS_SETTING_SIZE as u32 | (1 << 16);

impl NvdrsSetting {
    fn boxed() -> Box<Self> {
        // Tudo zero e um estado valido para essa struct C.
        let mut b: Box<Self> = unsafe { Box::new(std::mem::zeroed()) };
        b.version = NVDRS_SETTING_VER;
        b
    }
}

type Handle = *mut c_void;
type FnInitialize = unsafe extern "C" fn() -> i32;
type FnCreateSession = unsafe extern "C" fn(*mut Handle) -> i32;
type FnSession = unsafe extern "C" fn(Handle) -> i32;
type FnGetProfile = unsafe extern "C" fn(Handle, *mut Handle) -> i32;
type FnGetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut NvdrsSetting) -> i32;
type FnSetSetting = unsafe extern "C" fn(Handle, Handle, *mut NvdrsSetting) -> i32;
type FnProfileSetting = unsafe extern "C" fn(Handle, Handle, u32) -> i32;

struct Api {
    create_session: FnCreateSession,
    destroy_session: FnSession,
    load_settings: FnSession,
    save_settings: FnSession,
    get_current_global_profile: FnGetProfile,
    get_setting: FnGetSetting,
    set_setting: FnSetSetting,
    restore_default_setting: FnProfileSetting,
}

// Ponteiros de funcao do driver: imutaveis depois de carregados.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn load_api() -> Result<Api, String> {
    unsafe {
        let name = to_wide_null("nvapi64.dll");
        // So System32: evita carregar uma nvapi64.dll plantada na pasta atual.
        let module = LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if module.is_null() {
            return Err("Driver NVIDIA nao encontrado (nvapi64.dll ausente)".into());
        }
        let Some(qi) = GetProcAddress(module, c"nvapi_QueryInterface".as_ptr().cast()) else {
            return Err("nvapi_QueryInterface ausente em nvapi64.dll".into());
        };
        type FnQuery = unsafe extern "C" fn(u32) -> *mut c_void;
        let query = std::mem::transmute::<unsafe extern "system" fn() -> isize, FnQuery>(qi);
        let get = |id: u32| -> Result<*mut c_void, String> {
            let p = query(id);
            if p.is_null() {
                Err(format!("Funcao NVAPI 0x{id:08X} indisponivel neste driver"))
            } else {
                Ok(p)
            }
        };
        type P = *mut c_void;
        let initialize = std::mem::transmute::<P, FnInitialize>(get(ID_INITIALIZE)?);
        let st = initialize();
        if st != NVAPI_OK {
            return Err(format!("NvAPI_Initialize falhou (status {st}): sem GPU NVIDIA ativa?"));
        }
        Ok(Api {
            create_session: std::mem::transmute::<P, FnCreateSession>(get(ID_DRS_CREATE_SESSION)?),
            destroy_session: std::mem::transmute::<P, FnSession>(get(ID_DRS_DESTROY_SESSION)?),
            load_settings: std::mem::transmute::<P, FnSession>(get(ID_DRS_LOAD_SETTINGS)?),
            save_settings: std::mem::transmute::<P, FnSession>(get(ID_DRS_SAVE_SETTINGS)?),
            get_current_global_profile: std::mem::transmute::<P, FnGetProfile>(get(
                ID_DRS_GET_CURRENT_GLOBAL_PROFILE,
            )?),
            get_setting: std::mem::transmute::<P, FnGetSetting>(get(ID_DRS_GET_SETTING)?),
            set_setting: std::mem::transmute::<P, FnSetSetting>(get(ID_DRS_SET_SETTING)?),
            restore_default_setting: std::mem::transmute::<P, FnProfileSetting>(get(
                ID_DRS_RESTORE_PROFILE_DEFAULT_SETTING,
            )?),
        })
    }
}

fn api() -> Result<&'static Api, String> {
    API.get_or_init(load_api).as_ref().map_err(|e| e.clone())
}

/// Sessao DRS com o perfil global atual (o que o Painel de Controle NVIDIA mostra em
/// "Configuracoes globais"). Fecha a sessao no Drop.
struct Session {
    api: &'static Api,
    session: Handle,
    profile: Handle,
}

impl Session {
    fn open() -> Result<Self, String> {
        let api = api()?;
        unsafe {
            let mut session: Handle = std::ptr::null_mut();
            let st = (api.create_session)(&mut session);
            if st != NVAPI_OK || session.is_null() {
                return Err(format!("NvAPI_DRS_CreateSession falhou (status {st})"));
            }
            let mut me = Self { api, session, profile: std::ptr::null_mut() };
            let st = (api.load_settings)(session);
            if st != NVAPI_OK {
                return Err(format!("NvAPI_DRS_LoadSettings falhou (status {st})"));
            }
            let st = (api.get_current_global_profile)(session, &mut me.profile);
            if st != NVAPI_OK || me.profile.is_null() {
                return Err(format!("NvAPI_DRS_GetCurrentGlobalProfile falhou (status {st})"));
            }
            Ok(me)
        }
    }

    /// Ok(None) = setting nao definido no perfil (vale o padrao do driver).
    fn read(&self, id: u32) -> Result<Option<ReadValue>, String> {
        let mut s = NvdrsSetting::boxed();
        let st = unsafe { (self.api.get_setting)(self.session, self.profile, id, &mut *s) };
        if st == NVAPI_SETTING_NOT_FOUND {
            return Ok(None);
        }
        if st != NVAPI_OK {
            return Err(format!("NvAPI_DRS_GetSetting(0x{id:08X}) falhou (status {st})"));
        }
        if s.setting_type != NVDRS_DWORD_TYPE {
            return Err(format!("Setting 0x{id:08X} nao e DWORD (tipo {})", s.setting_type));
        }
        Ok(Some(ReadValue {
            value: s.current[0],
            is_predefined: s.is_current_predefined != 0,
        }))
    }

    fn write(&self, id: u32, value: u32) -> Result<(), String> {
        let mut s = NvdrsSetting::boxed();
        s.setting_id = id;
        s.setting_type = NVDRS_DWORD_TYPE;
        s.current[0] = value;
        let st = unsafe { (self.api.set_setting)(self.session, self.profile, &mut *s) };
        if st != NVAPI_OK {
            return Err(format!("NvAPI_DRS_SetSetting(0x{id:08X}) falhou (status {st})"));
        }
        Ok(())
    }

    fn restore_default(&self, id: u32) -> Result<(), String> {
        let st = unsafe { (self.api.restore_default_setting)(self.session, self.profile, id) };
        // Sem valor de usuario para restaurar tambem e o estado padrao.
        if st != NVAPI_OK && st != NVAPI_SETTING_NOT_FOUND {
            return Err(format!("NvAPI_DRS_RestoreProfileDefaultSetting(0x{id:08X}) falhou (status {st})"));
        }
        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        let st = unsafe { (self.api.save_settings)(self.session) };
        if st != NVAPI_OK {
            return Err(format!("NvAPI_DRS_SaveSettings falhou (status {st}); execute como Administrador"));
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            (self.api.destroy_session)(self.session);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadValue {
    pub value: u32,
    /// true = valor predefinido pelo driver (o usuario nunca alterou).
    pub is_predefined: bool,
}

/// Leitura somente dos 4 settings alvo no perfil global atual.
pub fn read_targets() -> Result<Vec<(u32, Option<ReadValue>)>, String> {
    let s = Session::open()?;
    TARGETS.iter().map(|&(id, _, _)| s.read(id).map(|v| (id, v))).collect()
}

/// Estado original de um setting. user_value = None: o perfil nao tinha valor do usuario
/// (padrao do driver), entao o revert usa RestoreProfileDefaultSetting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DrsBackupEntry {
    id: u32,
    user_value: Option<u32>,
}

fn snapshot_entry(id: u32, read: Option<ReadValue>) -> DrsBackupEntry {
    DrsBackupEntry {
        id,
        user_value: read.filter(|r| !r.is_predefined).map(|r| r.value),
    }
}

fn restore_entries(entries: &[DrsBackupEntry]) -> Result<(), String> {
    let s = Session::open()?;
    for e in entries {
        match e.user_value {
            Some(v) => s.write(e.id, v)?,
            None => s.restore_default(e.id)?,
        }
    }
    s.save()
}

pub struct NvidiaDrsProfileTweak {
    meta: TweakMeta,
}

impl NvidiaDrsProfileTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.nvidia_profile_latency",
                name: "GPU NVIDIA: Perfil global do driver (Low Latency On, desempenho maximo, cache de shader ilimitado)",
                category: Category::GpuLatency,
                risk: RiskLevel::Medium,
                requires_reboot: false,
                requires_admin: true,
                description: "Grava no perfil global do driver (o mesmo de Painel de Controle NVIDIA > Configuracoes globais) os valores do guia PC-Tuning: Low Latency Mode On (1 quadro pre-renderizado; jogos com NVIDIA Reflex ignoram), Power management mode = Prefer maximum performance, Shader Cache Size = Unlimited. (Ansel nao entra: drivers atuais nao suportam mais o setting.) Usa a NVAPI oficial carregada de System32; nada e injetado em jogos. Backup dos valores originais; o revert volta cada um ao valor anterior ou ao padrao do driver.",
                details: TARGETS
                    .iter()
                    .map(|&(id, v, label)| format!("DRS 0x{id:08X} = 0x{v:X} ({label})"))
                    .collect(),
            },
        }
    }
}

impl Tweak for NvidiaDrsProfileTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if let Err(e) = api() {
            return TweakStatus::NotApplicable(e);
        }
        let values = match read_targets() {
            Ok(v) => v,
            Err(e) => return TweakStatus::Error(e),
        };
        let ok = values
            .iter()
            .zip(TARGETS)
            .filter(|((_, read), &(_, target, _))| read.map(|r| r.value) == Some(target))
            .count();
        match ok {
            n if n == TARGETS.len() => TweakStatus::Applied,
            0 => TweakStatus::NotApplied,
            n => TweakStatus::Partial(format!("{n}/{} settings", TARGETS.len())),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        api()?;
        let before = read_targets()?;
        let snapshot: Vec<DrsBackupEntry> =
            before.iter().map(|&(id, read)| snapshot_entry(id, read)).collect();
        let json = serde_json::to_value(&snapshot).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let result = (|| -> Result<(), String> {
            let s = Session::open()?;
            for &(id, value, _) in TARGETS {
                s.write(id, value)?;
            }
            s.save()?;
            drop(s);
            // Confere relendo numa sessao nova: o valor gravado tem que ser o lido do driver.
            for ((id, read), &(_, target, label)) in read_targets()?.iter().zip(TARGETS) {
                if read.map(|r| r.value) != Some(target) {
                    return Err(format!("Driver nao confirmou 0x{id:08X} ({label})"));
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => Ok(format!("{} settings gravados no perfil global NVIDIA.", TARGETS.len())),
            Err(e) => {
                let rb = restore_entries(&snapshot);
                if created && rb.is_ok() {
                    let _ = backups.remove(self.meta.id);
                }
                match rb {
                    Ok(()) => Err(format!("{e} (reversao automatica concluida)")),
                    Err(re) => Err(format!("{e}; falha tambem na reversao: {re}")),
                }
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup do perfil NVIDIA encontrado.".into());
        };
        let entries: Vec<DrsBackupEntry> = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup do perfil NVIDIA invalido: {e}"))?;
        restore_entries(&entries)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} settings do perfil global NVIDIA restaurados.", entries.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let entries: Vec<DrsBackupEntry> = TARGETS
            .iter()
            .map(|&(id, _, _)| DrsBackupEntry { id, user_value: None })
            .collect();
        restore_entries(&entries)?;
        Ok("Settings do perfil global NVIDIA voltaram ao padrao do driver.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvdrs_setting_layout_matches_official_header() {
        // nvapi.h: 4 + 2048*2 + 5*4 + 4100 + 4100 com pack(4).
        assert_eq!(std::mem::size_of::<NvdrsSetting>(), NVDRS_SETTING_SIZE);
        assert_eq!(std::mem::align_of::<NvdrsSetting>(), 4);
        assert_eq!(NVDRS_SETTING_VER, 0x0001_3020);
        assert_eq!(std::mem::offset_of!(NvdrsSetting, setting_id), 4100);
        assert_eq!(std::mem::offset_of!(NvdrsSetting, predefined), 4120);
        assert_eq!(std::mem::offset_of!(NvdrsSetting, current), 8220);
    }

    #[test]
    fn snapshot_keeps_only_user_values_for_revert() {
        let user = ReadValue { value: 1, is_predefined: false };
        let predefined = ReadValue { value: 5, is_predefined: true };
        assert_eq!(snapshot_entry(7, Some(user)).user_value, Some(1));
        assert_eq!(snapshot_entry(7, Some(predefined)).user_value, None);
        assert_eq!(snapshot_entry(7, None).user_value, None);
    }

    #[test]
    fn targets_match_pc_tuning_values() {
        let ids: Vec<u32> = TARGETS.iter().map(|t| t.0).collect();
        assert_eq!(ids, [PRERENDERLIMIT_ID, PREFERRED_PSTATE_ID, PS_SHADERDISKCACHE_MAX_SIZE_ID]);
        assert_eq!(TARGETS[0].1, 1); // PRERENDERLIMIT = 1
        assert_eq!(TARGETS[1].1, 1); // PREFERRED_PSTATE_PREFER_MAX
        assert_eq!(TARGETS[2].1, 0xFFFF_FFFF); // PS_SHADERDISKCACHE_MAX_SIZE_MAX
    }

    /// Grava e rele na MESMA sessao sem SaveSettings: valida SetSetting/estrutura no driver real
    /// sem alterar o perfil salvo. Manual: cargo test drs_write_roundtrip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn drs_write_roundtrip_without_save() {
        let s = Session::open().expect("sessao DRS");
        for &(id, value, label) in TARGETS {
            s.write(id, value).expect(label);
            let back = s.read(id).expect("releitura").expect("valor presente");
            assert_eq!(back.value, value, "{label}");
            println!("OK sem salvar: 0x{id:08X} = 0x{value:X} ({label})");
        }
        // Drop fecha a sessao sem SaveSettings: nada persiste.
    }

    #[test]
    fn live_read_only_probe_of_global_profile() {
        match read_targets() {
            Ok(values) => {
                for (id, v) in values {
                    println!("0x{id:08X} -> {v:?}");
                }
            }
            Err(e) => println!("sem NVAPI neste ambiente: {e}"),
        }
    }
}
