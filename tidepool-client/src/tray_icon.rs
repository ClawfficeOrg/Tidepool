//! Platform abstraction for the system tray icon.
//!
//! Each platform provides a `TrayIcon` with the same interface:
//! - `TrayIcon::new(tooltip)` — create and register the tray icon.
//! - `TrayIcon::poll_click()` — check for a pending left-click event.
//! - `TrayIcon::poll_context_menu()` — check for a pending menu selection.
//! - `TrayIcon::set_state_color(r, g, b)` — recolour the icon per app state.
//! - `TrayIcon::set_menu(items)` — install the native context menu.
//! - `TrayIcon::anchor()` — where the icon is on screen, for window anchoring.
//!
//! On Linux (no libappindicator dependency) these are all no-ops and
//! `anchor` returns `None`.

// ── macOS implementation ───────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc::{self, Receiver, Sender};

    use objc2::ClassType;
    // `DefinedClass` carries `ivars()` and `AnyThread` carries `alloc()`; both
    // are plain trait methods in objc2 0.6, so they must be in scope.
    use objc2::define_class;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::sel;
    use objc2::{AnyThread, DefinedClass, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSColor, NSImage, NSMenu, NSMenuItem,
        NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSData, NSSize, NSString};

    use crate::icon_assets::PNG_BYTES;

    /// A click event dispatched from the status item.
    ///
    /// On macOS this is never constructed — left-clicks open the native menu
    /// and dispatch through `ContextMenuEvent` instead. The type exists only
    /// to satisfy the cross-platform `poll_click()` signature.
    #[derive(Debug)]
    pub struct ClickEvent;

    /// A context-menu selection, carrying the id of the chosen menu item.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContextMenuEvent {
        pub item_id: u32,
    }

    /// Ivars for the menu-target class: the sender that forwards a selection
    /// back to the Rust poll loop.
    struct MenuTargetIvars {
        sender: Sender<ContextMenuEvent>,
    }

    define_class!(
        /// Native menu target. Each menu item's `action:` fires this method
        /// with the clicked `NSMenuItem`; its `tag` carries the item id.
        #[unsafe(super(NSObject))]
        #[ivars = MenuTargetIvars]
        struct MenuTarget;

        impl MenuTarget {
            #[unsafe(method(action:))]
            fn action(&self, sender: &AnyObject) {
                let tag: isize = unsafe { msg_send![sender, tag] };
                let _ = self
                    .ivars()
                    .sender
                    .send(ContextMenuEvent { item_id: tag as u32 });
            }
        }
    );

    impl MenuTarget {
        fn new(sender: Sender<ContextMenuEvent>) -> Retained<Self> {
            let this = Self::alloc().set_ivars(MenuTargetIvars { sender });
            // SAFETY: `this` is a freshly allocated MenuTarget; initializing it
            // through the superclass init is the standard objc2 pattern.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// macOS-specific tray icon using `NSStatusBar`.
    ///
    /// Must be created on the main thread.
    pub struct TrayIconInner {
        _status_item: Retained<NSStatusItem>,
        /// The native menu, retained so it outlives the status item.
        _menu: Option<Retained<NSMenu>>,
        /// The menu action target, retained so items keep dispatching.
        _menu_target: Option<Retained<MenuTarget>>,
        menu_receiver: Receiver<ContextMenuEvent>,
        menu_sender: Sender<ContextMenuEvent>,
    }

    impl TrayIconInner {
        /// Creates a new tray icon in the macOS menu bar.
        ///
        /// # Panics (in tests only)
        /// Panics if called off the main thread when `MainThreadMarker::new()` fails.
        pub fn new(tooltip: &str) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;

            let app = unsafe { NSApplication::sharedApplication(mtm) };
            unsafe {
                app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }

            let status_bar = NSStatusBar::systemStatusBar();
            let status_item =
                unsafe { status_bar.statusItemWithLength(NSVariableStatusItemLength) };

            if let Some(button) = unsafe { status_item.button(mtm) } {
                let tip = NSString::from_str(tooltip);
                unsafe { button.setToolTip(Some(&tip)) };
                if let Some(image) = embedded_icon_image() {
                    unsafe { button.setImage(Some(&image)) };
                }
            }

            let (menu_sender, menu_receiver) = mpsc::channel::<ContextMenuEvent>();

            Some(Self {
                _status_item: status_item,
                _menu: None,
                _menu_target: None,
                menu_receiver,
                menu_sender,
            })
        }

        /// On macOS, left-click shows the native menu (not a separate event).
        /// Returns `None` always — use `poll_context_menu` for menu selections.
        pub fn poll_click(&self) -> Option<ClickEvent> {
            None
        }

        /// Returns the next pending context-menu selection, if any.
        pub fn poll_context_menu(&self) -> Option<ContextMenuEvent> {
            self.menu_receiver.try_recv().ok()
        }

        /// Second-instance surface requests are Windows-only; macOS surfaces
        /// the app through the native status-item menu instead.
        pub fn poll_show_request(&self) -> bool {
            false
        }

        /// Recolours the status-item icon.
        ///
        /// The embedded artwork is a template image, and AppKit tints template
        /// images through the button's `contentTintColor` — so this sets a tint
        /// rather than drawing a replacement swatch, and the icon keeps its
        /// shape. AppKit honours the tint in combination with the standard
        /// menubar effects, so it stays legible in both appearances. Passing
        /// `None` would restore the default (untinted) template rendering.
        pub fn set_state_color(&mut self, r: u8, g: u8, b: u8) {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let Some(button) = (unsafe { self._status_item.button(mtm) }) else {
                return;
            };
            let color = unsafe {
                NSColor::colorWithSRGBRed_green_blue_alpha(
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                    1.0,
                )
            };
            unsafe { button.setContentTintColor(Some(&color)) };
        }

        /// Installs the native context menu on the status item.
        ///
        /// On macOS a status item with a menu shows it for both left and right
        /// clicks (native menu-bar behaviour), so the left-click popover toggle
        /// is Windows-only.
        pub fn set_menu(&mut self, items: &[(u32, String)]) {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let target = MenuTarget::new(self.menu_sender.clone());
            let (menu, target) = build_menu(items, target, mtm);
            self._status_item.setMenu(Some(&menu));
            self._menu = Some(menu);
            self._menu_target = Some(target);
        }

        /// Returns the status item button's rectangle relative to the top-left
        /// of the screen it is on (see [`Self::display_id`]), used to anchor
        /// the popover window.
        ///
        /// Display-relative, not global: that is the space GPUI's macOS
        /// window bounds are in. It adds the target screen's origin itself.
        pub fn screen_rect(&self) -> Option<(f32, f32, f32, f32)> {
            let mtm = MainThreadMarker::new()?;
            let button = unsafe { self._status_item.button(mtm) }?;
            let frame = unsafe { button.frame() };

            let window = unsafe { button.window() }?;
            let screen = unsafe { window.screen() }?;
            let screen_frame = unsafe { screen.frame() };

            // button.frame() is in the status-item window's coordinates, and
            // the window's frame is in global AppKit coordinates (bottom-left
            // origin, y up). Take both relative to the screen, then flip y
            // against the screen's own top edge.
            let window_frame = unsafe { window.frame() };
            let x = (window_frame.origin.x + frame.origin.x - screen_frame.origin.x) as f32;
            let button_top = window_frame.origin.y + frame.origin.y + frame.size.height;
            let screen_top = screen_frame.origin.y + screen_frame.size.height;
            let y = (screen_top - button_top) as f32;
            let w = frame.size.width as f32;
            let h = frame.size.height as f32;

            Some((x, y, w, h))
        }

        /// Returns the id of the display the status item is on (its
        /// `CGDirectDisplayID`, which is what GPUI's `DisplayId` wraps).
        ///
        /// Each display has its own menu bar, so this is the display whose
        /// icon was clicked.
        pub fn display_id(&self) -> Option<u64> {
            let mtm = MainThreadMarker::new()?;
            let button = unsafe { self._status_item.button(mtm) }?;
            let window = unsafe { button.window() }?;
            let screen = unsafe { window.screen() }?;

            let key = NSString::from_str("NSScreenNumber");
            let description: Retained<AnyObject> =
                unsafe { msg_send![&*screen, deviceDescription] };
            let number: Option<Retained<AnyObject>> =
                unsafe { msg_send![&*description, objectForKey: &*key] };
            let id: u32 = unsafe { msg_send![&*number?, unsignedIntValue] };
            Some(u64::from(id))
        }
    }

    /// Builds the native `NSMenu` for the given items, wiring each entry to
    /// `target` through the `action:` selector. Item ids are stored as tags.
    fn build_menu(
        items: &[(u32, String)],
        target: Retained<MenuTarget>,
        mtm: MainThreadMarker,
    ) -> (Retained<NSMenu>, Retained<MenuTarget>) {
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for (id, label) in items {
            let title = NSString::from_str(label);
            let key = NSString::from_str("");
            // `NSMenuItem` is `MainThreadOnly`, so its `alloc` takes the marker;
            // the init borrows its strings rather than taking ownership.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &title,
                    Some(sel!(action:)),
                    &key,
                )
            };
            // SAFETY: `target` is retained for the menu's lifetime, so the weak
            // target reference remains valid while the menu is shown.
            unsafe {
                item.setTarget(Some(&*target));
            }
            item.setTag(*id as isize);
            menu.addItem(&item);
        }
        (menu, target)
    }

    /// Builds the menubar image from the icon embedded in the binary.
    ///
    /// Returns `None` if the bytes are not a decodable image, so a bad asset
    /// degrades to "no icon drawn" rather than a crash. The image is marked as a
    /// template so AppKit tints it for light and dark menubars — the artwork is
    /// a solid mark and would otherwise disappear against a matching menubar.
    pub(super) fn embedded_icon_image() -> Option<Retained<NSImage>> {
        let data =
            unsafe { NSData::dataWithBytes_length(PNG_BYTES.as_ptr().cast(), PNG_BYTES.len()) };
        let image = unsafe { NSImage::initWithData(NSImage::alloc(), &data) }?;
        // A menubar icon is ~16 pt tall. `initWithData` uses the file's own
        // pixel size, so pin the logical size instead of trusting the asset's
        // dimensions.
        unsafe { image.setSize(NSSize::new(16.0, 16.0)) };
        unsafe { image.setTemplate(true) };
        Some(image)
    }
}

// ── Windows implementation ─────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use std::sync::OnceLock;
    use std::sync::mpsc::{self, Receiver, Sender};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
        NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, GetCursorPos, HICON,
        HWND_MESSAGE, RegisterClassExW, RegisterWindowMessageW, WM_APP, WM_CONTEXTMENU,
        WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_EX_NOACTIVATE,
    };
    use windows::core::{PCWSTR, w};

    use crate::icon_assets::ICO_BYTES;

    const TRAY_UID: u32 = 1;
    const WM_TRAY_CALLBACK: u32 = WM_APP + 1;
    /// Custom message sent by a second instance to ask the first to surface
    /// its window. Defined here and duplicated in `lifecycle.rs` (the two
    /// modules cannot share a constant across cfg-gated compilation units).
    pub(crate) const SHOW_WINDOW_MSG: u32 = WM_APP + 2;
    const ICON_SIZE: i32 = 32;

    /// A click event dispatched from the tray icon callback window.
    #[derive(Debug)]
    pub struct ClickEvent;

    /// A context-menu selection, carrying the id of the chosen menu item.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContextMenuEvent {
        pub item_id: u32,
    }

    static TASKBAR_CREATED_MSG: OnceLock<u32> = OnceLock::new();

    /// Windows-specific tray icon using `Shell_NotifyIconW`.
    pub struct TrayIconInner {
        hwnd: HWND,
        /// The currently displayed icon, released on replacement and on drop.
        icon: Option<HICON>,
        receiver: Receiver<ClickEvent>,
        menu_receiver: Receiver<ContextMenuEvent>,
        /// Set when a second instance asks the first to surface its window.
        /// Only read by `poll_show_request`, which itself is only exercised
        /// through the binary (`main.rs`) — hence `dead_code`.
        #[allow(dead_code)]
        show_request_receiver: Receiver<()>,
    }

    impl TrayIconInner {
        /// Creates a hidden message-only HWND and registers the tray icon.
        ///
        /// Returns `None` if window creation or icon registration fails.
        pub fn new(tooltip: &str) -> Option<Self> {
            let (sender, receiver) = mpsc::channel::<ClickEvent>();
            let (menu_sender, menu_receiver) = mpsc::channel::<ContextMenuEvent>();
            let (surface_sender, show_request_receiver) = mpsc::channel::<()>();

            let taskbar_msg = TASKBAR_CREATED_MSG
                .get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });

            // Prefer the app's embedded icon; fall back to a neutral swatch so a
            // bad or missing asset still yields a visible tray icon.
            let icon = embedded_icon().or_else(|| create_colored_icon(140, 140, 140));
            let hwnd = create_message_window(
                sender,
                menu_sender,
                surface_sender,
                *taskbar_msg,
                tooltip.to_string(),
            )?;

            let mut nid = build_notify_icon_data(hwnd, tooltip, icon)?;
            let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() };
            if !ok {
                unsafe { DestroyWindow(hwnd).ok() };
                if let Some(icon) = icon {
                    unsafe { DestroyIcon(icon).ok() };
                }
                return None;
            }

            // Enable VERSION_4 behavior (GUID support, NIN_* notifications).
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &nid) };

            Some(Self {
                hwnd,
                icon,
                receiver,
                menu_receiver,
                show_request_receiver,
            })
        }

        /// Returns the next pending click event, if any.
        pub fn poll_click(&self) -> Option<ClickEvent> {
            self.receiver.try_recv().ok()
        }

        /// Returns whether a second instance asked the first to surface its
        /// window. Only exercised through the binary's `pump_tray`.
        #[allow(dead_code)]
        pub fn poll_show_request(&self) -> bool {
            self.show_request_receiver.try_recv().is_ok()
        }

        /// Returns the next pending context-menu selection, if any.
        pub fn poll_context_menu(&self) -> Option<ContextMenuEvent> {
            self.menu_receiver.try_recv().ok()
        }

        /// Replaces the tray icon with a solid-colour icon of the given RGB.
        pub fn set_state_color(&mut self, r: u8, g: u8, b: u8) {
            let Some(new_icon) = create_colored_icon(r, g, b) else {
                return;
            };
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_UID,
                uFlags: NIF_ICON,
                hIcon: new_icon,
                ..Default::default()
            };
            let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() };
            if ok {
                if let Some(old) = self.icon.replace(new_icon) {
                    unsafe { DestroyIcon(old).ok() };
                }
            } else {
                unsafe { DestroyIcon(new_icon).ok() };
            }
        }

        /// Stores the context-menu definition for the tray callback window.
        pub fn set_menu(&mut self, items: &[(u32, String)]) {
            use windows::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW};
            // SAFETY: the WindowState pointer is set in WM_CREATE and remains
            // valid until WM_NCDESTROY; both run on the same thread.
            let ptr = unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) } as *mut WindowState;
            if !ptr.is_null() {
                unsafe { (*ptr).menu = items.to_vec() };
            }
        }

        /// Returns the screen rectangle for the tray icon (top-left origin,
        /// matching GPUI's `Bounds<Pixels>` convention).
        ///
        /// Returns `None` if the icon is hidden in the overflow area.
        pub fn screen_rect(&self) -> Option<(f32, f32, f32, f32)> {
            use windows::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
            use windows::core::GUID;

            let identifier = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_UID,
                guidItem: GUID::zeroed(),
            };
            let rect = match unsafe { Shell_NotifyIconGetRect(&identifier) } {
                Ok(rect) => rect,
                Err(_) => return None,
            };
            Some((
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
            ))
        }

        /// Returns `None` — the rectangle is global on Windows, so the
        /// display is found from it by geometry.
        pub fn display_id(&self) -> Option<u64> {
            None
        }
    }

    impl Drop for TrayIconInner {
        fn drop(&mut self) {
            // Remove tray icon before destroying the window and icon.
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_UID,
                ..Default::default()
            };
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
            if let Some(icon) = self.icon.take() {
                unsafe { DestroyIcon(icon).ok() };
            }
            unsafe { DestroyWindow(self.hwnd).ok() };
        }
    }

    fn build_notify_icon_data(
        hwnd: HWND,
        tooltip: &str,
        icon: Option<HICON>,
    ) -> Option<NOTIFYICONDATAW> {
        let mut u_flags = NIF_MESSAGE | NIF_TIP;
        if icon.is_some() {
            u_flags |= NIF_ICON;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            uFlags: u_flags,
            hIcon: icon.unwrap_or(HICON(std::ptr::null_mut())),
            uCallbackMessage: WM_TRAY_CALLBACK,
            ..Default::default()
        };

        // Encode tooltip as UTF-16; truncate to fit szTip (128 u16 code units).
        let utf16: Vec<u16> = tooltip.encode_utf16().take(127).collect();
        for (dst, src) in nid.szTip.iter_mut().zip(utf16.iter()) {
            *dst = *src;
        }

        Some(nid)
    }

    fn create_message_window(
        sender: Sender<ClickEvent>,
        menu_sender: Sender<ContextMenuEvent>,
        surface_sender: Sender<()>,
        taskbar_msg: u32,
        tooltip: String,
    ) -> Option<HWND> {
        // Box the state so we can pass a raw pointer through CREATESTRUCT.
        // SAFETY: the box is reconstructed and re-leaked in WM_CREATE, then
        // dropped on WM_NCDESTROY.
        let state = Box::new(WindowState {
            sender,
            menu_sender,
            surface_sender,
            taskbar_msg,
            tooltip,
            menu: Vec::new(),
        });

        let class_name = w!("ZoidTrayWindowClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        // Ignore error — class may already be registered in the same process.
        unsafe { RegisterClassExW(&wc) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                PCWSTR(class_name.as_ptr()),
                w!(""),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        };

        hwnd.ok()
    }

    struct WindowState {
        sender: Sender<ClickEvent>,
        menu_sender: Sender<ContextMenuEvent>,
        surface_sender: Sender<()>,
        taskbar_msg: u32,
        tooltip: String,
        menu: Vec<(u32, String)>,
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        use windows::Win32::UI::Shell::NIN_SELECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GWLP_USERDATA, GetWindowLongPtrW, SetWindowLongPtrW, WM_CREATE, WM_NCDESTROY,
        };

        if msg == WM_CREATE {
            // SAFETY: lparam is a pointer to the CREATESTRUCT passed by the OS
            // in WM_CREATE, valid for the duration of the message.
            let cs = unsafe {
                &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
            };
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
            return LRESULT(0);
        }

        // SAFETY: GWLP_USERDATA holds the pointer set in WM_CREATE.
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;

        if msg == WM_NCDESTROY {
            if !ptr.is_null() {
                // SAFETY: ptr was created via Box::into_raw in create_message_window
                // and has not been dropped before WM_NCDESTROY.
                unsafe {
                    drop(Box::from_raw(ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            return LRESULT(0);
        }

        if ptr.is_null() {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }

        // SAFETY: ptr is non-null and valid between WM_CREATE and WM_NCDESTROY.
        let state = unsafe { &*ptr };

        if msg == WM_TRAY_CALLBACK {
            let notification = lparam.0 as u32 & 0xFFFF;
            match notification {
                NIN_SELECT | WM_LBUTTONUP => {
                    let _ = state.sender.send(ClickEvent);
                }
                WM_CONTEXTMENU | WM_RBUTTONUP => {
                    let mut point = POINT::default();
                    if unsafe { GetCursorPos(&mut point) }.is_ok() {
                        show_context_menu(hwnd, state, point.x, point.y);
                    }
                }
                _ => {}
            }
        } else if msg == state.taskbar_msg {
            // Explorer restarted — re-add the tray icon, using the same
            // embedded icon (falling back like `new` does).
            let Some(icon) = embedded_icon().or_else(|| create_colored_icon(140, 140, 140)) else {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            };
            let Some(mut nid) = build_notify_icon_data(hwnd, &state.tooltip, Some(icon)) else {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            };
            unsafe {
                let _ = Shell_NotifyIconW(NIM_ADD, &nid);
                nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
            }
        } else if msg == SHOW_WINDOW_MSG {
            // A second instance is asking us to surface our window. Routed
            // through its own channel so it is not conflated with a real
            // left-click (which toggles the tray popover).
            let _ = state.surface_sender.send(());
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// Shows the popup context menu at the given screen position and forwards
    /// the selected item id through the menu channel.
    fn show_context_menu(hwnd: HWND, state: &WindowState, x: i32, y: i32) {
        use windows::Win32::UI::WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD,
            TrackPopupMenu,
        };

        if state.menu.is_empty() {
            return;
        }

        let menu = match unsafe { CreatePopupMenu() } {
            Ok(menu) => menu,
            Err(_) => return,
        };

        let mut appended = false;
        for (id, label) in &state.menu {
            // AppendMenuW copies the label text, so a temporary buffer is fine.
            let wide: Vec<u16> = format!("{label}\0").encode_utf16().collect();
            if unsafe { AppendMenuW(menu, MF_STRING, *id as usize, PCWSTR(wide.as_ptr())) }.is_ok()
            {
                appended = true;
            }
        }
        if !appended {
            unsafe { DestroyMenu(menu).ok() };
            return;
        }

        // TPM_RETURNCMD makes TrackPopupMenu return the selected item id;
        // TPM_NONOTIFY keeps the menu from stealing focus from the app.
        let selected =
            unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, x, y, None, hwnd, None) };
        unsafe { DestroyMenu(menu).ok() };

        let item_id = selected.0;
        if item_id != 0 {
            let _ = state.menu_sender.send(ContextMenuEvent {
                item_id: item_id as u32,
            });
        }
    }

    /// Builds an `HICON` from the icon embedded in the binary.
    ///
    /// `.ico` files hold a directory of images at several sizes; the shell wants
    /// one `HICON`, so this picks the entry closest to [`ICON_SIZE`] and hands
    /// that single image to `CreateIconFromResourceEx`. Returns `None` on a
    /// malformed or empty file so a bad asset degrades to the fallback icon
    /// rather than crashing the app at launch.
    pub(super) fn embedded_icon() -> Option<HICON> {
        use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, IMAGE_FLAGS};

        let image = best_ico_image(ICO_BYTES, ICON_SIZE)?;

        // SAFETY: `image` borrows from the static embedded bytes, which outlive
        // the call; the returned HICON is owned by the caller.
        unsafe {
            let result = CreateIconFromResourceEx(
                image,
                true,
                // 0x00030000 is the icon resource version Windows expects.
                0x0003_0000,
                ICON_SIZE,
                ICON_SIZE,
                // LR_DEFAULTCOLOR
                IMAGE_FLAGS(0),
            );
            result.ok()
        }
    }

    /// Picks the image whose width is closest to `target` from an `.ico` file
    /// and returns its bytes.
    ///
    /// The ICO layout is a 6-byte header, then a 16-byte directory entry per
    /// image, then the image data. A width of `0` in an entry means 256px.
    fn best_ico_image(ico: &'static [u8], target: i32) -> Option<&'static [u8]> {
        if ico.len() < 6 {
            return None;
        }
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        if count == 0 || ico.len() < 6 + count * 16 {
            return None;
        }

        let mut best: Option<(i32, &'static [u8])> = None;
        for index in 0..count {
            let entry = &ico[6 + index * 16..6 + index * 16 + 16];
            let width = if entry[0] == 0 { 256 } else { entry[0] as i32 };
            let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
            let end = offset.checked_add(size)?;
            if end > ico.len() {
                continue;
            }
            let distance = (width - target).abs();
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, &ico[offset..end]));
            }
        }
        best.map(|(_, data)| data)
    }

    /// Builds a solid-colour `HICON` for the given RGB, used by
    /// [`TrayIconInner::set_state_color`].
    pub(super) fn create_colored_icon(r: u8, g: u8, b: u8) -> Option<HICON> {
        use windows::Win32::Foundation::TRUE;
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
            DeleteObject, HGDIOBJ, SetBitmapBits,
        };
        use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

        // SAFETY: every handle created below is released before returning and
        // no handle escapes this function's scope.
        unsafe {
            // 32bpp top-down DIB section with an explicit alpha channel. The
            // shell alpha-blends 32bpp icons, so alpha MUST be 255 — a plain
            // CreateCompatibleBitmap + FillRect path leaves alpha at 0, which
            // renders the icon fully transparent (registered, but invisible).
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: ICON_SIZE,
                    biHeight: -ICON_SIZE,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let color = match CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bmp) => bmp,
                Err(_) => return None,
            };
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(color.0));
                return None;
            }

            // BGRA little-endian pixel: B | G<<8 | R<<16 | A<<24.
            let pixel = (r as u32) << 16 | (g as u32) << 8 | b as u32 | 0xFF00_0000;
            let px =
                std::slice::from_raw_parts_mut(bits as *mut u32, (ICON_SIZE * ICON_SIZE) as usize);
            px.fill(pixel);

            // Monochrome AND-mask: all-zero bits make the icon fully opaque.
            let mask = CreateBitmap(ICON_SIZE, ICON_SIZE, 1, 1, None);
            if !mask.0.is_null() {
                let zeros = vec![0u8; (ICON_SIZE * ICON_SIZE / 8) as usize];
                SetBitmapBits(mask, zeros.len() as u32, zeros.as_ptr().cast());
            }

            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: TRUE,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            });

            let _ = DeleteObject(HGDIOBJ(mask.0));
            let _ = DeleteObject(HGDIOBJ(color.0));

            icon.ok()
        }
    }
}

// ── Linux / other platforms ────────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    /// Click event — unused on unsupported platforms.
    #[derive(Debug)]
    pub struct ClickEvent;

    /// A context-menu selection, carrying the id of the chosen menu item.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContextMenuEvent {
        pub item_id: u32,
    }

    /// Stub tray icon for platforms without system-tray support.
    pub struct TrayIconInner;

    impl TrayIconInner {
        /// Always returns `None` on unsupported platforms and emits a warning.
        pub fn new(_tooltip: &str) -> Option<Self> {
            eprintln!(
                "warning: tray icon not supported on this platform — opening a normal window"
            );
            None
        }

        /// Returns `None` — no tray icon exists.
        pub fn poll_click(&self) -> Option<ClickEvent> {
            None
        }

        /// Returns `None` — no tray icon exists.
        pub fn poll_context_menu(&self) -> Option<ContextMenuEvent> {
            None
        }

        /// Returns `false` — no second-instance surface request exists.
        pub fn poll_show_request(&self) -> bool {
            false
        }

        /// No-op — no tray icon exists.
        pub fn set_state_color(&mut self, _r: u8, _g: u8, _b: u8) {}

        /// No-op — no tray icon exists.
        pub fn set_menu(&mut self, _items: &[(u32, String)]) {}

        /// Returns `None` — no screen rectangle available.
        pub fn screen_rect(&self) -> Option<(f32, f32, f32, f32)> {
            None
        }

        /// Returns `None` — no tray icon exists.
        pub fn display_id(&self) -> Option<u64> {
            None
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

use platform::TrayIconInner;

pub use platform::ContextMenuEvent;

/// Where the tray icon is on screen, for anchoring a window to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconAnchor {
    /// `(x, y, width, height)` of the icon, top-left origin. Relative to
    /// `display` when that is set (macOS), otherwise in global screen
    /// coordinates (Windows). Either way it is the space GPUI reports that
    /// platform's display bounds in.
    pub rect: (f32, f32, f32, f32),
    /// Raw id of the display the icon is on, when the platform has to name
    /// it. macOS reports every display's bounds at origin `(0, 0)`, so the
    /// display cannot be found from the rectangle there.
    pub display: Option<u64>,
}

impl IconAnchor {
    /// The display the icon is on: the one it names, else the one whose
    /// bounds contain the icon's centre. `None` when neither matches a live
    /// display.
    ///
    /// Pass this to `WindowOptions::display_id` (and `Bounds::centered`) so
    /// a window opened from the tray appears on the display the user
    /// clicked. Without it GPUI opens on the primary display.
    pub fn display_id(&self, cx: &gpui::App) -> Option<gpui::DisplayId> {
        let displays = cx.displays();
        if let Some(id) = self.display.map(gpui::DisplayId::new)
            && displays.iter().any(|display| display.id() == id)
        {
            return Some(id);
        }
        let (x, y, w, h) = self.rect;
        let center = gpui::point(gpui::px(x + w / 2.0), gpui::px(y + h / 2.0));
        displays
            .iter()
            .find(|display| display.bounds().contains(&center))
            .map(|display| display.id())
    }
}

/// Cross-platform system tray icon.
///
/// Create with [`TrayIcon::new`] on the main thread.
pub struct TrayIcon {
    inner: Option<TrayIconInner>,
}

impl TrayIcon {
    /// Creates a new tray icon with the given tooltip.
    ///
    /// Returns a `TrayIcon` whose `inner` is `None` on unsupported platforms.
    pub fn new(tooltip: &str) -> Self {
        Self {
            inner: TrayIconInner::new(tooltip),
        }
    }

    /// Returns the next pending left-click event, if any.
    #[allow(dead_code)]
    pub fn poll_click(&self) -> bool {
        self.inner.as_ref().and_then(|i| i.poll_click()).is_some()
    }

    /// Returns the next pending context-menu selection, if any.
    #[allow(dead_code)]
    pub fn poll_context_menu(&self) -> Option<ContextMenuEvent> {
        self.inner.as_ref().and_then(|i| i.poll_context_menu())
    }

    /// Returns whether a second instance asked the first to surface its
    /// window (Windows-only; always `false` elsewhere). Only exercised
    /// through the binary's `pump_tray`.
    #[allow(dead_code)]
    pub fn poll_show_request(&self) -> bool {
        self.inner.as_ref().is_some_and(|i| i.poll_show_request())
    }

    /// Recolours the icon to the given RGB.
    pub fn set_state_color(&mut self, r: u8, g: u8, b: u8) {
        if let Some(inner) = self.inner.as_mut() {
            inner.set_state_color(r, g, b);
        }
    }

    /// Installs the native context menu, as `(item_id, label)` pairs. Item ids
    /// are reported back through [`TrayIcon::poll_context_menu`].
    #[allow(dead_code)]
    pub fn set_menu(&mut self, items: &[(u32, String)]) {
        if let Some(inner) = self.inner.as_mut() {
            inner.set_menu(items);
        }
    }

    /// Returns where the icon is on screen, or `None` if unavailable.
    pub fn anchor(&self) -> Option<IconAnchor> {
        let inner = self.inner.as_ref()?;
        Some(IconAnchor {
            rect: inner.screen_rect()?,
            display: inner.display_id(),
        })
    }
}

impl gpui::Global for TrayIcon {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_new_does_not_panic() {
        // On macOS/Windows this may return None if not on the main thread;
        // that is expected and correct — the important thing is no panic.
        let mut icon = TrayIcon::new("Test Tooltip");
        let _ = icon.poll_click();
        let _ = icon.poll_context_menu();
        icon.set_state_color(10, 20, 30);
        icon.set_menu(&[(0, "Item".to_string())]);
        let _ = icon.anchor();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn platform_inner_is_none_on_linux() {
        let icon = TrayIcon::new("tooltip");
        assert!(icon.inner.is_none(), "Linux inner must be None");
    }
}
