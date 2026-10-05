//! Intégration avec le shell Windows : menu contextuel officiel,
//! Propriétés, « Ouvrir avec » et presse-papiers partagé avec l'Explorateur.

use std::cell::RefCell;
use windows::core::{w, Interface, Result, HSTRING, PCSTR, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_MOVE: u32 = 2;
const DROPEFFECT_LINK: u32 = 4;
const CMIC_MASK_UNICODE: u32 = 0x4000;

fn shell_items(paths: &[String]) -> Result<IShellItemArray> {
    unsafe {
        let pidls: Vec<*mut ITEMIDLIST> =
            paths.iter().map(|p| ILCreateFromPathW(&HSTRING::from(p.as_str()))).collect();
        let result = if pidls.is_empty() || pidls.iter().any(|p| p.is_null()) {
            Err(E_FAIL.into())
        } else {
            let refs: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| *p as *const _).collect();
            SHCreateShellItemArrayFromIDLists(&refs)
        };
        for p in pidls {
            if !p.is_null() {
                ILFree(Some(p));
            }
        }
        result
    }
}

thread_local! {
    // Menu affiché en ce moment : nécessaire pour remplir les sous-menus
    // dynamiques (« Ouvrir avec », « Envoyer vers »...).
    static ACTIVE_MENU: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        let handled = ACTIVE_MENU.with(|m| {
            let m = m.borrow();
            let cm = m.as_ref()?;
            if let Ok(cm3) = cm.cast::<IContextMenu3>() {
                let mut res = LRESULT(0);
                if cm3.HandleMenuMsg2(msg, wp, lp, Some(&mut res)).is_ok() {
                    return Some(res);
                }
            } else if let Ok(cm2) = cm.cast::<IContextMenu2>() {
                if cm2.HandleMenuMsg(msg, wp, lp).is_ok() {
                    return Some(LRESULT(0));
                }
            }
            None
        });
        if let Some(r) = handled {
            return r;
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

/// Affiche le menu contextuel officiel de Windows à la position de la souris.
pub fn context_menu(hwnd: HWND, paths: &[String]) -> Result<()> {
    unsafe {
        let items = shell_items(paths)?;
        let cm: IContextMenu = items.BindToHandler(None, &BHID_SFUIObject)?;
        let menu = CreatePopupMenu()?;
        cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL | CMF_EXPLORE).ok()?;

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        ACTIVE_MENU.with(|m| *m.borrow_mut() = Some(cm.clone()));
        let _ = SetWindowSubclass(hwnd, Some(menu_proc), 1, 0);
        let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, pt.x, pt.y, hwnd, None);
        let _ = RemoveWindowSubclass(hwnd, Some(menu_proc), 1);
        ACTIVE_MENU.with(|m| *m.borrow_mut() = None);

        let id = cmd.0;
        let result = if id > 0 {
            let offset = (id - 1) as usize;
            let info = CMINVOKECOMMANDINFOEX {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
                hwnd,
                lpVerb: PCSTR(offset as *const u8),
                lpVerbW: PCWSTR(offset as *const u16),
                nShow: SW_SHOWNORMAL.0,
                ptInvoke: pt,
                ..Default::default()
            };
            cm.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO)
        } else {
            Ok(())
        };
        let _ = DestroyMenu(menu);
        result
    }
}

/// Fenêtre « Propriétés » de Windows (un ou plusieurs éléments).
pub fn properties(paths: &[String]) -> Result<()> {
    unsafe {
        let items = shell_items(paths)?;
        let data: IDataObject = items.BindToHandler(None, &BHID_DataObject)?;
        SHMultiFileProperties(&data, 0)
    }
}

/// Boîte de dialogue « Ouvrir avec » de Windows.
pub fn open_with(hwnd: HWND, path: &str) -> Result<()> {
    unsafe {
        let file = HSTRING::from(path);
        let info = OPENASINFO {
            pcszFile: PCWSTR(file.as_ptr()),
            pcszClass: PCWSTR::null(),
            oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_EXEC,
        };
        SHOpenWithDialog(Some(hwnd), &info)
    }
}

/// Place des fichiers dans le presse-papiers Windows (collables dans l'Explorateur).
pub fn clipboard_set(hwnd: HWND, paths: &[String], cut: bool) -> Result<()> {
    unsafe {
        let mut wide: Vec<u16> = Vec::new();
        for p in paths {
            wide.extend(p.encode_utf16());
            wide.push(0);
        }
        wide.push(0);

        let header = std::mem::size_of::<DROPFILES>();
        let files = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, header + wide.len() * 2)?;
        let ptr = GlobalLock(files) as *mut u8;
        let df = ptr as *mut DROPFILES;
        (*df).pFiles = header as u32;
        (*df).fWide = true.into();
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr.add(header), wide.len() * 2);
        let _ = GlobalUnlock(files);

        let effect = GlobalAlloc(GMEM_MOVEABLE, 4)?;
        let p = GlobalLock(effect) as *mut u32;
        *p = if cut { DROPEFFECT_MOVE } else { DROPEFFECT_COPY | DROPEFFECT_LINK };
        let _ = GlobalUnlock(effect);

        OpenClipboard(Some(hwnd))?;
        let _ = EmptyClipboard();
        let r = SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(files.0)));
        let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
        let _ = SetClipboardData(fmt, Some(HANDLE(effect.0)));
        let _ = CloseClipboard();
        r.map(|_| ())
    }
}

/// Lit les fichiers présents dans le presse-papiers Windows.
pub fn clipboard_get(hwnd: HWND) -> Result<(Vec<String>, bool)> {
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let mut paths = Vec::new();
        let mut cut = false;
        if let Ok(h) = GetClipboardData(CF_HDROP.0 as u32) {
            let hdrop = HDROP(h.0);
            let n = DragQueryFileW(hdrop, u32::MAX, None);
            for i in 0..n {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                paths.push(String::from_utf16_lossy(&buf[..len]));
            }
            let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            if let Ok(h2) = GetClipboardData(fmt) {
                let g = HGLOBAL(h2.0);
                let p = GlobalLock(g) as *const u32;
                if !p.is_null() {
                    cut = *p & DROPEFFECT_MOVE != 0 && *p & DROPEFFECT_COPY == 0;
                    let _ = GlobalUnlock(g);
                }
            }
        }
        let _ = CloseClipboard();
        Ok((paths, cut))
    }
}

/// Vide le presse-papiers (après un « couper / coller », comme l'Explorateur).
pub fn clipboard_clear(hwnd: HWND) -> Result<()> {
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let _ = EmptyClipboard();
        CloseClipboard()
    }
}

/* ---------------- Opérations de fichiers (moteur de l'Explorateur) ---------------- */

pub enum Op {
    Recycle,
    Copy,
    Move,
}

/// Corbeille / copie / déplacement via IFileOperation, exactement comme l'Explorateur :
/// fenêtre de progression, « Remplacer ou ignorer », « fichier utilisé », annulation (Ctrl+Z
/// dans l'Explorateur). À appeler depuis un fil dédié. Renvoie `true` si quelque chose a été annulé.
pub fn file_op(owner: isize, op: Op, paths: &[String], dest: Option<&str>, rename_on_collision: bool) -> Result<bool> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| -> Result<bool> {
            let fo: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            let mut flags = FOF_ALLOWUNDO | FOFX_ADDUNDORECORD;
            match op {
                Op::Recycle => flags |= FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING | FOFX_RECYCLEONDELETE,
                _ if rename_on_collision => flags |= FOF_RENAMEONCOLLISION,
                _ => {}
            }
            fo.SetOperationFlags(flags)?;
            fo.SetOwnerWindow(HWND(owner as *mut _))?;
            let items = shell_items(paths)?;
            match op {
                Op::Recycle => fo.DeleteItems(&items)?,
                Op::Copy | Op::Move => {
                    let target: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(dest.unwrap_or_default()), None)?;
                    if matches!(op, Op::Copy) {
                        fo.CopyItems(&items, &target)?;
                    } else {
                        fo.MoveItems(&items, &target)?;
                    }
                }
            }
            let performed = fo.PerformOperations();
            let aborted = fo.GetAnyOperationsAborted().map(|b| b.as_bool()).unwrap_or(false);
            match performed {
                Ok(()) => Ok(aborted),
                // Annulation par l'utilisateur : pas une erreur
                Err(e) if aborted || e.code().0 as u32 == 0x8027_0000 => Ok(true),
                Err(e) => Err(e),
            }
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

/* ---------------- Glisser-déposer ---------------- */

/// Fait passer une fenêtre au premier plan, même pendant un glisser-déposer
/// (passage temporaire « toujours visible », que Windows autorise sans restriction).
pub fn raise(hwnd: HWND, activate: bool) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW;
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
        let _ = SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags);
        if activate {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Fenêtres à ne jamais faire passer devant : barre des tâches, Bureau, image du glisser.
fn ignored_window(h: HWND) -> bool {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    let class = String::from_utf16_lossy(&buf[..n]);
    matches!(
        class.as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW" | "SysDragImage" | "NotifyIconOverflowWindow" | "XamlExplorerHostIslandWindow"
    )
}

/// Pendant un glisser : la fenêtre survolée (n'importe quel logiciel) passe au premier plan
/// après un court instant, pour voir où l'on dépose. S'arrête quand `stop` passe à vrai.
fn watch_drag_hover(stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    std::thread::spawn(move || {
        let mut last: isize = 0;
        let mut since = Instant::now();
        let mut raised: isize = 0;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(40));
            unsafe {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_err() {
                    continue;
                }
                let root = GetAncestor(WindowFromPoint(pt), GA_ROOT);
                let id = root.0 as isize;
                if id == 0 {
                    continue;
                }
                if id != last {
                    last = id;
                    since = Instant::now();
                } else if id != raised
                    && since.elapsed() >= Duration::from_millis(450)
                    && GetForegroundWindow() != root
                    && !ignored_window(root)
                {
                    raise(root, true);
                    raised = id;
                }
            }
        }
    });
}

/// Démarre un glisser-déposer Windows (vers le Bureau, l'Explorateur, d'autres logiciels...).
pub fn start_drag(hwnd: HWND, paths: &[String]) -> Result<()> {
    use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
    let stop = Arc::new(AtomicBool::new(false));
    watch_drag_hover(stop.clone());
    let r = start_drag_inner(hwnd, paths);
    stop.store(true, Ordering::Relaxed);
    r
}

fn start_drag_inner(hwnd: HWND, paths: &[String]) -> Result<()> {
    unsafe {
        // Autorise la fenêtre de destination (autre fenêtre Kane) à passer au premier plan
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let items = shell_items(paths)?;
        let data: IDataObject = items.BindToHandler(None, &BHID_DataObject)?;
        let all = DROPEFFECT(DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK);
        SHDoDragDrop(Some(hwnd), &data, None, all).map(|_| ())
    }
}

/// Touches Ctrl / Maj enfoncées (pour choisir copier ou déplacer au dépôt).
pub fn modifiers() -> (bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
    unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) < 0, GetAsyncKeyState(VK_SHIFT.0 as i32) < 0) }
}

/* ---------------- Miniatures Windows ---------------- */

/// Miniature (ou icône) d'un fichier via le moteur de Windows, encodée en PNG.
pub fn thumbnail_png(path: &str, size: i32, thumb_only: bool, cache_only: bool) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None)?;
        let mut flags = SIIGBF_BIGGERSIZEOK;
        if thumb_only {
            flags |= SIIGBF_THUMBNAILONLY;
        }
        if cache_only {
            flags |= SIIGBF_THUMBNAILONLY | SIIGBF_INCACHEONLY;
        }
        // Windows répond parfois « pas encore prêt » (E_PENDING) : on réessaie brièvement
        let mut attempt = 0;
        let hbmp = loop {
            match factory.GetImage(SIZE { cx: size, cy: size }, flags) {
                Err(e) if e.code() == windows::core::HRESULT(0x8000000A_u32 as i32) && attempt < 20 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                r => break r?,
            }
        };
        let png = bitmap_to_png(hbmp);
        let _ = DeleteObject(hbmp.into());
        png
    }
}

unsafe fn bitmap_to_png(hbmp: windows::Win32::Graphics::Gdi::HBITMAP) -> Result<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::*;
    let mut bm = BITMAP::default();
    GetObjectW(hbmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 {
        return Err(E_FAIL.into());
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // de haut en bas
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let hdc = GetDC(None);
    let lines = GetDIBits(hdc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
    ReleaseDC(None, hdc);
    if lines == 0 {
        return Err(E_FAIL.into());
    }
    // BGRA (alpha prémultiplié) -> RGBA
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        } else if p[3] > 0 && p[3] < 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().map_err(|_| windows::core::Error::from(E_FAIL))?;
        writer.write_image_data(&buf).map_err(|_| windows::core::Error::from(E_FAIL))?;
    }
    Ok(out)
}

/* ---------------- Aperçu natif (Word, Excel, PowerPoint...) ---------------- */

const PREVIEW_HANDLER: PCWSTR = w!("{8895b1c6-b41f-4c1c-a562-0d564250836f}");

/// Module d'aperçu Windows enregistré pour cette extension, s'il existe.
pub fn preview_handler_clsid(ext: &str) -> Option<windows::core::GUID> {
    use windows::core::PWSTR;
    use windows::Win32::System::Com::CLSIDFromString;
    unsafe {
        let assoc = HSTRING::from(format!(".{ext}"));
        let mut buf = [0u16; 64];
        let mut len = buf.len() as u32;
        AssocQueryStringW(ASSOCF_NONE, ASSOCSTR_SHELLEXTENSION, &assoc, PREVIEW_HANDLER, Some(PWSTR(buf.as_mut_ptr())), &mut len)
            .ok()
            .ok()?;
        CLSIDFromString(PCWSTR(buf.as_ptr())).ok()
    }
}

struct Native {
    host: HWND,
    parent: HWND,
    handler: IPreviewHandler,
}

/// Coordonnées dans la fenêtre de Kane -> coordonnées écran.
fn to_screen(parent: HWND, x: i32, y: i32) -> POINT {
    let mut pt = POINT { x, y };
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(parent, &mut pt);
    }
    pt
}

thread_local! {
    static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
}

/// Affiche l'aperçu officiel de Windows dans un rectangle de la fenêtre (pixels physiques).
pub fn native_preview_show(parent: HWND, path: &str, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
    use windows::Win32::Foundation::{E_NOINTERFACE, RECT};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, STGM_READ, STGM_SHARE_DENY_NONE};
    use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithFile, IInitializeWithStream};
    native_preview_close();
    unsafe {
        let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let clsid = preview_handler_clsid(ext).ok_or(windows::core::Error::from(E_FAIL))?;
        let wpath = HSTRING::from(path);
        // Crée le module puis l'initialise avec la première méthode qu'il accepte
        let open = |ctx| -> Result<IPreviewHandler> {
            let handler: IPreviewHandler = CoCreateInstance(&clsid, None, ctx)?;
            let mut res: Result<()> = Err(E_NOINTERFACE.into());
            if let Ok(i) = handler.cast::<IInitializeWithFile>() {
                res = i.Initialize(&wpath, STGM_READ.0);
            }
            if res.is_err() {
                if let Ok(i) = handler.cast::<IInitializeWithItem>() {
                    res = SHCreateItemFromParsingName::<_, _, IShellItem>(&wpath, None)
                        .and_then(|item| i.Initialize(&item, STGM_READ.0));
                }
            }
            if res.is_err() {
                if let Ok(i) = handler.cast::<IInitializeWithStream>() {
                    res = SHCreateStreamOnFileEx(&wpath, (STGM_READ | STGM_SHARE_DENY_NONE).0, 0, false, None)
                        .and_then(|s| i.Initialize(&s, STGM_READ.0));
                }
            }
            res.map(|_| handler)
        };
        // Petite fenêtre « collée » au-dessus de Kane : le moteur web dessine par-dessus
        // ses fenêtres enfants, une fenêtre détenue (owned) reste toujours visible.
        let pt = to_screen(parent, x, y);
        let host = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!(""),
            WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
            pt.x,
            pt.y,
            w,
            h,
            Some(parent),
            None,
            None,
            None,
        )?;
        let rc = RECT { left: 0, top: 0, right: w, bottom: h };
        // Hors processus d'abord (comme l'Explorateur : un module défaillant ne peut pas planter Kane)
        let mut last = Err(E_FAIL.into());
        for ctx in [CLSCTX_LOCAL_SERVER, CLSCTX_INPROC_SERVER] {
            let attempt = open(ctx).and_then(|handler| {
                match handler.SetWindow(host, &rc).and_then(|_| handler.DoPreview()) {
                    Ok(()) => {
                        // Certains modules ne prennent leur taille qu'après l'affichage
                        let _ = handler.SetRect(&rc);
                        fit_children(host, w, h);
                        Ok(handler)
                    }
                    Err(e) => {
                        let _ = handler.Unload();
                        Err(e)
                    }
                }
            });
            match attempt {
                Ok(handler) => {
                    NATIVE.with(|n| *n.borrow_mut() = Some(Native { host, parent, handler }));
                    return Ok(());
                }
                Err(e) => last = Err(e),
            }
        }
        let _ = DestroyWindow(host);
        last
    }
}

/// Étire la fenêtre du module d'aperçu à la taille du cadre (certains modules oublient de le faire).
unsafe fn fit_children(host: HWND, w: i32, h: i32) {
    if let Ok(child) = GetWindow(host, GW_CHILD) {
        let _ = SetWindowPos(child, None, 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn native_preview_move(x: i32, y: i32, w: i32, h: i32) {
    use windows::Win32::Foundation::RECT;
    NATIVE.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            unsafe {
                let pt = to_screen(n.parent, x, y);
                let _ = SetWindowPos(n.host, None, pt.x, pt.y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
                let _ = n.handler.SetRect(&RECT { left: 0, top: 0, right: w, bottom: h });
                fit_children(n.host, w, h);
            }
        }
    });
}

/// L'aperçu natif affiche-t-il réellement quelque chose (fenêtre du module non vide) ?
pub fn native_preview_alive() -> bool {
    use windows::Win32::Foundation::RECT;
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return false };
        unsafe {
            let Ok(child) = GetWindow(n.host, GW_CHILD) else { return false };
            let mut r = RECT::default();
            GetWindowRect(child, &mut r).is_ok() && r.right > r.left && r.bottom > r.top
        }
    })
}

pub fn native_preview_visible(visible: bool) {
    NATIVE.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            unsafe {
                let _ = ShowWindow(n.host, if visible { SW_SHOWNA } else { SW_HIDE });
            }
        }
    });
}

pub fn native_preview_close() {
    if let Some(n) = NATIVE.with(|n| n.borrow_mut().take()) {
        unsafe {
            let _ = n.handler.Unload();
            let _ = DestroyWindow(n.host);
        }
    }
}

/* ---------------- Fond d'écran, propriétés, registre ---------------- */

/// Définit une image comme fond d'écran Windows.
pub fn set_wallpaper(path: &str) -> Result<()> {
    let mut wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide.as_mut_ptr() as *mut _),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    }
}

/// Propriétés lues par Windows (durée, résolution, images/s, débit, codec, appareil photo...).
pub fn media_props(path: &str) -> Result<Vec<(String, String)>> {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Storage::EnhancedStorage::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PSFormatForDisplayAlloc, SHGetPropertyStoreFromParsingName, GPS_DEFAULT, PDFF_DEFAULT};
    let keys: [(&str, PROPERTYKEY); 16] = [
        ("Durée", PKEY_Media_Duration),
        ("Largeur", PKEY_Video_FrameWidth),
        ("Hauteur", PKEY_Video_FrameHeight),
        ("Images/s", PKEY_Video_FrameRate),
        ("Débit vidéo", PKEY_Video_EncodingBitrate),
        ("Débit total", PKEY_Video_TotalBitrate),
        ("Codec vidéo", PKEY_Video_FourCC),
        ("Débit audio", PKEY_Audio_EncodingBitrate),
        ("Fréquence", PKEY_Audio_SampleRate),
        ("Canaux", PKEY_Audio_ChannelCount),
        ("Titre", PKEY_Title),
        ("Artiste", PKEY_Music_Artist),
        ("Album", PKEY_Music_AlbumTitle),
        ("Appareil", PKEY_Photo_CameraModel),
        ("Prise de vue", PKEY_Photo_DateTaken),
        ("Dimensions", PKEY_Image_Dimensions),
    ];
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let store: IPropertyStore = SHGetPropertyStoreFromParsingName(&HSTRING::from(path), None, GPS_DEFAULT)?;
        let mut out = Vec::new();
        for (label, key) in keys {
            let Ok(value) = store.GetValue(&key) else { continue };
            if value.is_empty() {
                continue;
            }
            // Codec : code FourCC (ex. « H264 ») plutôt qu'un nombre
            if label == "Codec vidéo" {
                if let Ok(n) = u32::try_from(&value) {
                    let s: String = n.to_le_bytes().iter().map(|&b| b as char).collect();
                    if s.chars().all(|c| c.is_ascii_graphic()) {
                        out.push((label.to_string(), s.trim().to_string()));
                    }
                }
                continue;
            }
            if let Ok(text) = PSFormatForDisplayAlloc(&key, &value, PDFF_DEFAULT) {
                let s = text.to_string().unwrap_or_default();
                CoTaskMemFree(Some(text.0 as *const _));
                let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{200e}' || c == '\u{200f}').to_string();
                if !s.is_empty() {
                    out.push((label.to_string(), s));
                }
            }
        }
        Ok(out)
    }
}

/// Lance PowerShell en administrateur (Windows affiche sa demande d'autorisation).
pub fn run_elevated_powershell(script: &str) -> Result<()> {
    let params = HSTRING::from(format!("-NoProfile -WindowStyle Hidden -Command \"{}\"", script.replace('"', "\\\"")));
    let r = unsafe { ShellExecuteW(None, w!("runas"), w!("powershell.exe"), &params, PCWSTR::null(), SW_HIDE) };
    // ShellExecute renvoie une valeur > 32 en cas de succès (refus de l'utilisateur = échec)
    if r.0 as isize > 32 { Ok(()) } else { Err(windows::core::Error::from(E_FAIL)) }
}

/* ----- Réseau : détection des cartes branchées / débranchées ----- */

/// Empreinte des cartes réseau matérielles (nom + état). Lecture native, instantanée :
/// sert à détecter un branchement (ex. adaptateur USB) sans relancer PowerShell en boucle.
pub fn network_signature() -> String {
    use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).is_err() || table.is_null() {
            return String::new();
        }
        let t = &*table;
        let rows = std::slice::from_raw_parts(t.Table.as_ptr(), t.NumEntries as usize);
        let mut sig = String::new();
        for r in rows {
            // Bit 0 : interface matérielle (exclut les cartes virtuelles, tunnels...)
            if r.InterfaceAndOperStatusFlags._bitfield & 1 == 0 {
                continue;
            }
            let n = r.Alias.iter().position(|&c| c == 0).unwrap_or(r.Alias.len());
            sig.push_str(&String::from_utf16_lossy(&r.Alias[..n]));
            sig.push_str(&format!(":{}:{};", r.OperStatus.0, r.AdminStatus.0));
        }
        FreeMibTable(table as *const _);
        sig
    }
}

/* ----- Noms affichés et dossiers connus ----- */

/// Nom affiché par l'Explorateur (traduction via desktop.ini, ex. « Captures d'écran »).
pub fn display_name(path: &str) -> Option<String> {
    let mut info = SHFILEINFOW::default();
    let ok = unsafe {
        SHGetFileInfoW(
            &HSTRING::from(path),
            Default::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_DISPLAYNAME,
        )
    };
    if ok == 0 {
        return None;
    }
    let n = info.szDisplayName.iter().position(|&c| c == 0).unwrap_or(0);
    (n > 0).then(|| String::from_utf16_lossy(&info.szDisplayName[..n]))
}

/// Emplacement réel du dossier « Captures d'écran » (peut être dans OneDrive).
pub fn known_folder_screenshots() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Screenshots, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(std::path::PathBuf::from)
    }
}

/* ----- Registre de l'utilisateur (HKCU) ----- */

/// Écrit une valeur texte (`name` = None : valeur par défaut de la clé), en créant la clé si besoin.
pub fn reg_set(subkey: &str, name: Option<&str>, value: &str) -> Result<()> {
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let name = name.map(HSTRING::from);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
            REG_SZ.0,
            Some(data.as_ptr() as *const _),
            (data.len() * 2) as u32,
        )
        .ok()
    }
}

/// Lit une valeur texte (`name` = None : valeur par défaut).
pub fn reg_get(subkey: &str, name: Option<&str>) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let name = name.map(HSTRING::from);
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
        .ok()
        .ok()?;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n]))
}

/// Supprime une valeur (`name` = None : valeur par défaut).
pub fn reg_delete_value(subkey: &str, name: Option<&str>) {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    let name = name.map(HSTRING::from);
    unsafe {
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
        );
    }
}

/// Supprime une clé et tout son contenu.
pub fn reg_delete_tree(subkey: &str) {
    use windows::Win32::System::Registry::{RegDeleteKeyW, RegDeleteTreeW, HKEY_CURRENT_USER};
    let k = HSTRING::from(subkey);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &k);
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &k);
    }
}

/// Dossier d'installation de Steam (registre).
pub fn steam_path() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    unsafe {
        let mut buf = [0u16; 520];
        let mut len = (buf.len() * 2) as u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
        .ok()
        .ok()?;
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        Some(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]).replace('/', "\\")))
    }
}

#[cfg(test)]
mod tests {
    /// Écriture / lecture / suppression dans une clé temporaire (aucune clé réelle touchée).
    #[test]
    fn registre_aller_retour() {
        let key = r"Software\KaneRegTest\Directory\shell\KaneExplorer\command";
        let cmd = r#""C:\Program Files\Kane\kane-explorer.exe" "%1""#;
        super::reg_set(key, None, cmd).unwrap();
        super::reg_set(key, Some("DelegateExecute"), "").unwrap();
        assert_eq!(super::reg_get(key, None).as_deref(), Some(cmd));
        assert_eq!(super::reg_get(key, Some("DelegateExecute")).as_deref(), Some(""));
        super::reg_delete_tree(r"Software\KaneRegTest");
        assert!(super::reg_get(key, None).is_none());
    }
    /// Noms traduits par Windows et dossier « Captures d'écran » (lecture seule).
    #[test]
    fn noms_traduits() {
        let shots = super::known_folder_screenshots().expect("dossier Captures d'écran");
        let name = super::display_name(&shots.to_string_lossy()).unwrap_or_default();
        println!("dossier : {} -> affiché : {}", shots.display(), name);
        assert!(!name.is_empty());
    }
}
