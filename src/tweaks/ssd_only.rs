//! Trava "so sem HD mecanico" para ajustes que o PC-Tuning (valleyofdoom) manda aplicar
//! apenas quando nao ha HDD: Prefetch/Superfetch aceleram carregamento em disco rotativo.
//! check/apply viram N/A/erro com HDD presente; revert passa direto para quem ja aplicou.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceSeekPenaltyProperty, DEVICE_SEEK_PENALTY_DESCRIPTOR,
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::sys::backup::BackupStore;
use crate::sys::elevation::to_wide_null;
use crate::tweaks::{Tweak, TweakMeta, TweakStatus};

static HDD_PRESENT: OnceLock<Option<bool>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriveProbe {
    /// Nao abriu: nao ha disco neste indice.
    Missing,
    /// Abriu, mas a consulta falhou: disco existe com tipo desconhecido.
    Unknown,
    Solid,
    Rotational,
}

/// Mesma consulta (seek penalty) que o otimizador de unidades do Windows usa; nao depende do
/// smphost/Get-PhysicalDisk, que o proprio catalogo pode desativar.
fn probe_drive(index: u32) -> DriveProbe {
    let path = to_wide_null(&format!(r"\\.\PhysicalDrive{index}"));
    unsafe {
        // Acesso 0: consulta de propriedade nao exige leitura do disco nem elevacao.
        let handle = CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return DriveProbe::Missing;
        }
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceSeekPenaltyProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut desc: DEVICE_SEEK_PENALTY_DESCRIPTOR = std::mem::zeroed();
        let mut returned = 0u32;
        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const c_void,
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            &mut desc as *mut _ as *mut c_void,
            std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            &mut returned,
            null_mut(),
        );
        CloseHandle(handle);
        if ok == 0 || (returned as usize) < std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() {
            return DriveProbe::Unknown;
        }
        if desc.IncursSeekPenalty != 0 {
            DriveProbe::Rotational
        } else {
            DriveProbe::Solid
        }
    }
}

/// Regra pura (testavel): Some(true) se algum disco for rotativo; None se algum disco existente
/// tiver tipo desconhecido ou nenhum disco for encontrado; Some(false) so com todos confirmados solidos.
fn summarize(probes: &[DriveProbe]) -> Option<bool> {
    if probes.contains(&DriveProbe::Rotational) {
        return Some(true);
    }
    if probes.contains(&DriveProbe::Unknown) || !probes.contains(&DriveProbe::Solid) {
        return None;
    }
    Some(false)
}

fn hdd_present() -> Option<bool> {
    *HDD_PRESENT.get_or_init(|| {
        let mut probes = Vec::new();
        let mut missing_in_row = 0u32;
        for index in 0..64u32 {
            let probe = probe_drive(index);
            if probe == DriveProbe::Missing {
                missing_in_row += 1;
                // A numeracao pode ter buracos (disco removido); 8 indices vazios seguidos = fim.
                if missing_in_row >= 8 {
                    break;
                }
                continue;
            }
            missing_in_row = 0;
            probes.push(probe);
        }
        summarize(&probes)
    })
}

pub struct SsdOnlyTweak {
    inner: Box<dyn Tweak>,
    meta: TweakMeta,
}

impl SsdOnlyTweak {
    pub fn new(inner: Box<dyn Tweak>) -> Self {
        let mut meta = inner.meta().clone();
        meta.details
            .push("Trava: N/A se houver HD mecanico (Get-PhysicalDisk MediaType=HDD)".into());
        Self { inner, meta }
    }
}

impl Tweak for SsdOnlyTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        match hdd_present() {
            Some(true) => TweakStatus::NotApplicable("HD mecanico presente: Prefetch ajuda o carregamento".into()),
            Some(false) => self.inner.check(),
            None => TweakStatus::Error("Nao foi possivel identificar os discos (Get-PhysicalDisk)".into()),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        match hdd_present() {
            Some(false) => self.inner.apply(backups),
            Some(true) => Err("HD mecanico presente: ajuste nao aplicado (Prefetch/Superfetch aceleram HDD).".into()),
            None => Err("Nao foi possivel confirmar que nao ha HD mecanico; ajuste nao aplicado.".into()),
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        self.inner.revert(backups)
    }

    fn can_reset_default(&self) -> bool {
        self.inner.can_reset_default()
    }

    fn reset_to_default(&self) -> Result<String, String> {
        self.inner.reset_to_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_only_reports_no_hdd_when_every_drive_is_confirmed_solid() {
        use DriveProbe::*;
        assert_eq!(summarize(&[Solid, Solid]), Some(false));
        assert_eq!(summarize(&[Solid, Rotational]), Some(true));
        assert_eq!(summarize(&[Unknown, Rotational]), Some(true));
        assert_eq!(summarize(&[Solid, Unknown]), None);
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn probe_finds_at_least_one_drive_on_this_machine() {
        let found: Vec<DriveProbe> = (0..8)
            .map(probe_drive)
            .filter(|p| *p != DriveProbe::Missing)
            .collect();
        assert!(!found.is_empty(), "nenhum PhysicalDrive encontrado");
        println!("discos: {found:?} -> hdd_present={:?}", hdd_present());
    }
}
