//! Win32 prototype view. All HWNDs and the borrowed model stay on the creating thread.
use crate::{
    Error, Result,
    client::ClientController,
    game_library::{GameLibrary, Installation, LibraryStore},
    launch_gate::LibraryLaunchState,
};
use std::{cell::RefCell, ffi::c_void, mem, path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{COLOR_WINDOW, DEFAULT_GUI_FONT, GetStockObject},
    System::{LibraryLoader::GetModuleHandleW, SystemServices::SS_LEFT},
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
            SetProcessDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::*,
    },
};

const GAMES: usize = 101;
const LAUNCH: usize = 102;
const STOP: usize = 103;
const RELOAD: usize = 104;

struct View {
    model: ClientController,
    store: LibraryStore,
    library: GameLibrary,
    list: HWND,
    status: HWND,
    launch: HWND,
    stop: HWND,
    reload: HWND,
    caption: HWND,
    detail: HWND,
    displayed: String,
    error: Option<String>,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub(crate) fn run(workspace: PathBuf, library_path: PathBuf) -> Result<()> {
    let store = LibraryStore::new(library_path);
    let library = store.load()?;
    let view = RefCell::new(View {
        model: ClientController::new(workspace),
        store,
        library,
        list: ptr::null_mut(),
        status: ptr::null_mut(),
        launch: ptr::null_mut(),
        stop: ptr::null_mut(),
        reload: ptr::null_mut(),
        caption: ptr::null_mut(),
        detail: ptr::null_mut(),
        displayed: String::new(),
        error: None,
    });
    let class = wide(&format!("EmuCoreLibrary{}", std::process::id()));
    // SAFETY: This function owns the message loop. `view` and the UTF-16 class name
    // outlive the window. The callback borrows the model only for a single message.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hIcon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
            hbrBackground: (COLOR_WINDOW as usize + 1) as _,
            ..mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(last_error("register client window"));
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("EmuCore | Game library").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            920,
            590,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            (&view as *const RefCell<View>).cast::<c_void>(),
        );
        if hwnd.is_null() {
            let error = last_error("create client window");
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        ShowWindow(hwnd, SW_SHOW);
        let mut message: MSG = mem::zeroed();
        let result = loop {
            let received = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
            if received == -1 {
                break Err(last_error("read client message"));
            }
            if received == 0 {
                break if message.wParam == 0 {
                    Ok(())
                } else {
                    Err(crate::Error::new(
                        crate::ErrorKind::Host,
                        "client window callback failed",
                    ))
                };
            }
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        };
        if IsWindow(hwnd) != 0 {
            DestroyWindow(hwnd);
        }
        UnregisterClassW(class.as_ptr(), instance);
        result
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Win32 supplies the callback arguments. Unwinding is caught before
    // returning through the ABI; all pointer dereferences are in dispatch.
    match std::panic::catch_unwind(|| unsafe { dispatch(hwnd, message, wparam, lparam) }) {
        Ok(result) => result,
        Err(_) => {
            // SAFETY: Posts a quit request to this callback's own GUI thread.
            unsafe {
                PostQuitMessage(1);
            }
            0
        }
    }
}

unsafe fn dispatch(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: The window was created with a live RefCell<View> owned by run's stack.
    // No ownership is transferred to Win32. WM_DESTROY clears userdata before that
    // stack can unwind. try_borrow_mut ignores synchronous reentrant notifications.
    unsafe {
        if message == WM_NCCREATE {
            let creation = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
        if message == WM_CLOSE {
            DestroyWindow(hwnd);
            return 0;
        }
        if message == WM_DESTROY {
            KillTimer(hwnd, 1);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
            return 0;
        }
        let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>;
        if !data.is_null()
            && let Ok(mut view) = (*data).try_borrow_mut()
        {
            match message {
                WM_CREATE => {
                    if let Err(error) = view.create(hwnd) {
                        view.error = Some(error.to_string());
                        return -1;
                    }
                    if SetTimer(hwnd, 1, 200, None) == 0 {
                        return -1;
                    }
                    return 0;
                }
                WM_SIZE => {
                    view.layout(hwnd);
                    return 0;
                }
                WM_TIMER => {
                    view.model.poll();
                    view.refresh();
                    return 0;
                }
                WM_COMMAND => {
                    let id = wparam & 0xffff;
                    let notification = (wparam >> 16) & 0xffff;
                    if id == LAUNCH || (id == GAMES && notification == LBN_DBLCLK as usize) {
                        view.launch_selected();
                    }
                    if id == STOP
                        && let Err(error) = view.model.stop()
                    {
                        view.error = Some(error.to_string());
                    }
                    if id == RELOAD {
                        match view.store.load() {
                            Ok(library) => {
                                view.library = library;
                                view.error = None;
                                view.fill_list();
                            }
                            Err(error) => view.error = Some(error.to_string()),
                        }
                    }
                    view.refresh();
                    return 0;
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

impl View {
    unsafe fn create(&mut self, hwnd: HWND) -> Result<()> {
        // SAFETY: All controls are children of the live parent on its own thread.
        unsafe {
            self.caption = control(hwnd, "STATIC", "YOUR GAME LIBRARY", SS_LEFT, 0)?;
            self.detail = control(
                hwnd,
                "STATIC",
                "Android starts when you choose an installed game. Installation status is cached from verification.",
                SS_LEFT,
                0,
            )?;
            self.list = control(
                hwnd,
                "LISTBOX",
                "",
                WS_TABSTOP
                    | WS_BORDER
                    | WS_VSCROLL
                    | LBS_NOTIFY as u32
                    | LBS_NOINTEGRALHEIGHT as u32,
                GAMES,
            )?;
            self.launch = control(
                hwnd,
                "BUTTON",
                "Launch selected",
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                LAUNCH,
            )?;
            self.stop = control(
                hwnd,
                "BUTTON",
                "Stop / cancel",
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                STOP,
            )?;
            self.reload = control(
                hwnd,
                "BUTTON",
                "Reload library",
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                RELOAD,
            )?;
            self.status = control(hwnd, "STATIC", "", SS_LEFT, 0)?;
            self.fill_list();
            self.layout(hwnd);
            self.refresh();
        }
        Ok(())
    }

    unsafe fn fill_list(&self) {
        // SAFETY: The listbox copies each synchronous UTF-16 string before return.
        unsafe {
            SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
            for game in &self.library.games {
                let state = match game.installation {
                    Installation::Installed { .. } => "Installed (cached)",
                    Installation::NotInstalled => "Not installed",
                };
                let profile = self
                    .library
                    .profile_for(game)
                    .map_or("Missing profile", |profile| profile.name.as_str());
                let title = wide(&format!("{}   |   {state}   |   {profile}", game.title));
                SendMessageW(self.list, LB_ADDSTRING, 0, title.as_ptr() as isize);
            }
            if self.library.games.is_empty() {
                SetWindowTextW(self.detail, wide("Your library is empty. Add a verified entry using the documented library setup. Android is stopped.").as_ptr());
            } else {
                SetWindowTextW(self.detail, wide("Double-click a game or choose Launch selected. Installed status is cached until Android confirms launch.").as_ptr());
            }
        }
    }

    unsafe fn layout(&self, hwnd: HWND) {
        // SAFETY: All positions apply only to this view's live child controls.
        unsafe {
            let mut rect = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let scale = GetDpiForWindow(hwnd) as i32;
            let px = |value: i32| value * scale / 96;
            let width = rect.right.max(px(420));
            let height = rect.bottom.max(px(300));
            MoveWindow(self.caption, px(24), px(22), width - px(48), px(28), 1);
            MoveWindow(self.detail, px(24), px(57), width - px(48), px(48), 1);
            MoveWindow(
                self.list,
                px(24),
                px(112),
                width - px(48),
                height - px(256),
                1,
            );
            let y = height - px(126);
            MoveWindow(self.launch, px(24), y, px(145), px(34), 1);
            MoveWindow(self.stop, px(181), y, px(145), px(34), 1);
            MoveWindow(self.reload, px(338), y, px(145), px(34), 1);
            MoveWindow(
                self.status,
                px(24),
                height - px(76),
                width - px(48),
                px(68),
                1,
            );
        }
    }

    unsafe fn launch_selected(&mut self) {
        // SAFETY: Synchronous query of this view's listbox, no pointer parameters.
        let index = unsafe { SendMessageW(self.list, LB_GETCURSEL, 0, 0) };
        if index < 0 {
            self.error = Some("Select a game first.".into());
            return;
        }
        let Some(game) = self.library.games.get(index as usize) else {
            return;
        };
        match self.model.launch(&self.library, &game.id) {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    unsafe fn refresh(&mut self) {
        let message = self
            .error
            .as_deref()
            .unwrap_or_else(|| self.model.message());
        // SAFETY: All calls are synchronous on the window's creating thread.
        unsafe {
            if self.displayed != message {
                SetWindowTextW(self.status, wide(message).as_ptr());
                self.displayed = message.to_owned();
            }
            let busy = matches!(
                self.model.state(),
                LibraryLaunchState::Starting { .. } | LibraryLaunchState::Stopping
            );
            EnableWindow(self.list, (!busy) as i32);
            EnableWindow(
                self.launch,
                (!busy && !self.library.games.is_empty()) as i32,
            );
            EnableWindow(self.reload, (!busy) as i32);
            EnableWindow(
                self.stop,
                (!matches!(
                    self.model.state(),
                    LibraryLaunchState::Idle | LibraryLaunchState::Stopping
                )) as i32,
            );
        }
    }
}

unsafe fn control(parent: HWND, class: &str, title: &str, style: u32, id: usize) -> Result<HWND> {
    // SAFETY: Class and title are live nul-terminated UTF-16 for this synchronous
    // call. Controls remain parent-owned. The stock font is borrowed, never deleted.
    unsafe {
        let child = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(title).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            parent,
            id as _,
            GetModuleHandleW(ptr::null()),
            ptr::null(),
        );
        if child.is_null() {
            return Err(last_error("create client control"));
        }
        SendMessageW(
            child,
            WM_SETFONT,
            GetStockObject(DEFAULT_GUI_FONT) as usize,
            1,
        );
        Ok(child)
    }
}

fn last_error(context: &str) -> Error {
    Error::io(context, std::io::Error::last_os_error())
}
