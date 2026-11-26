use std::ptr::{null, null_mut};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BringWindowToTop, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, EnumWindows, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, LoadIconW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow,
    TrackPopupMenu, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, IDI_APPLICATION, MF_STRING,
    MSG, SW_HIDE, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
    WM_USER, WNDCLASSW,
};

use super::elevation::to_wide_null;

const WM_TRAYICON: u32 = WM_USER + 1;
const ID_MENU_RESTORE: usize = 1001;
const ID_MENU_EXIT: usize = 1002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Restore,
    Exit,
}

type TrayCallback = Arc<dyn Fn(TrayAction) + Send + Sync>;

struct TrayWindowState {
    nid: NOTIFYICONDATAW,
    callback: TrayCallback,
}

pub struct TrayIcon {
    hwnd_raw: isize,
    thread: Option<JoinHandle<()>>,
}

struct EnumSearch {
    pid: u32,
    found: isize,
}

unsafe extern "system" fn enum_own_window_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam as *mut EnumSearch);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == ctx.pid && IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
        ctx.found = hwnd as isize;
        return 0;
    }
    1
}

pub fn find_own_visible_window() -> Option<isize> {
    let mut search = EnumSearch {
        pid: std::process::id(),
        found: 0,
    };
    unsafe {
        EnumWindows(
            Some(enum_own_window_cb),
            (&mut search as *mut EnumSearch) as LPARAM,
        );
    }
    if search.found != 0 {
        Some(search.found)
    } else {
        None
    }
}

pub fn hide_window(hwnd_raw: isize) {
    if hwnd_raw != 0 {
        unsafe {
            ShowWindow(hwnd_raw as HWND, SW_HIDE);
        }
    }
}

pub fn restore_window(hwnd_raw: isize) {
    if hwnd_raw != 0 {
        unsafe {
            let hwnd = hwnd_raw as HWND;
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
            BringWindowToTop(hwnd);
        }
    }
}

pub fn close_window(hwnd_raw: isize) {
    if hwnd_raw != 0 {
        unsafe {
            let hwnd = hwnd_raw as HWND;
            ShowWindow(hwnd, SW_SHOW);
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

impl TrayIcon {
    pub fn new(tooltip: &str, callback: TrayCallback) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Result<isize, String>>();
        let tip = tooltip.to_string();

        let thread = std::thread::spawn(move || unsafe {
            let hinstance = GetModuleHandleW(null());
            let class_name = to_wide_null("01LowOS_TrayHost_Wnd");

            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(tray_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            };
            RegisterClassW(&wc);

            let mut hicon = LoadIconW(hinstance, std::ptr::without_provenance::<u16>(1));
            if hicon.is_null() {
                hicon = LoadIconW(null_mut(), IDI_APPLICATION);
            }

            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAYICON;
            nid.hIcon = hicon;

            let wide_tip: Vec<u16> = tip.encode_utf16().collect();
            let max_len = nid.szTip.len().saturating_sub(1);
            let copy_len = wide_tip.len().min(max_len);
            nid.szTip[..copy_len].copy_from_slice(&wide_tip[..copy_len]);

            let state = Box::new(TrayWindowState { nid, callback });
            let state_ptr = Box::into_raw(state);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                hinstance,
                state_ptr as *const _,
            );

            if hwnd.is_null() {
                drop(Box::from_raw(state_ptr));
                let _ = tx.send(Err("Falha ao criar janela oculta da bandeja.".into()));
                return;
            }

            (*state_ptr).nid.hWnd = hwnd;
            if Shell_NotifyIconW(NIM_ADD, &(*state_ptr).nid) == 0 {
                DestroyWindow(hwnd);
                let _ = tx.send(Err("Shell_NotifyIconW(NIM_ADD) falhou.".into()));
                return;
            }

            let _ = tx.send(Ok(hwnd as isize));

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });

        let hwnd_raw = rx
            .recv()
            .map_err(|_| "Thread da bandeja encerrou inesperadamente.".to_string())??;
        Ok(Self {
            hwnd_raw,
            thread: Some(thread),
        })
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        if self.hwnd_raw != 0 {
            unsafe {
                PostMessageW(self.hwnd_raw as HWND, WM_CLOSE, 0, 0);
            }
        }
        if let Some(h) = self.thread.take() {
            if h.thread().id() != std::thread::current().id() {
                let _ = h.join();
            }
        }
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            0
        }
        WM_TRAYICON => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayWindowState;
            if !ptr.is_null() {
                let state = &*ptr;
                let event = (lparam as u32) & 0xFFFF;
                if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                    (state.callback)(TrayAction::Restore);
                } else if event == WM_RBUTTONUP {
                    let hmenu = CreatePopupMenu();
                    if !hmenu.is_null() {
                        let open_txt = to_wide_null("Abrir 01LowOS");
                        let exit_txt = to_wide_null("Sair do 01LowOS");
                        AppendMenuW(hmenu, MF_STRING, ID_MENU_RESTORE, open_txt.as_ptr());
                        AppendMenuW(hmenu, MF_STRING, ID_MENU_EXIT, exit_txt.as_ptr());

                        let mut pt = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut pt);
                        SetForegroundWindow(hwnd);
                        TrackPopupMenu(
                            hmenu,
                            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                            pt.x,
                            pt.y,
                            0,
                            hwnd,
                            null(),
                        );
                        PostMessageW(hwnd, WM_NULL, 0, 0);
                        DestroyMenu(hmenu);
                    }
                }
            }
            0
        }
        WM_COMMAND => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayWindowState;
            if !ptr.is_null() {
                let state = &*ptr;
                let cmd_id = wparam & 0xFFFF;
                if cmd_id == ID_MENU_RESTORE {
                    (state.callback)(TrayAction::Restore);
                } else if cmd_id == ID_MENU_EXIT {
                    (state.callback)(TrayAction::Exit);
                }
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayWindowState;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let state = Box::from_raw(ptr);
                Shell_NotifyIconW(NIM_DELETE, &state.nid);
                drop(state);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
