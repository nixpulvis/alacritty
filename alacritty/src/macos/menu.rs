use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSWorkspace};
use objc2_foundation::{NSString, NSURL, ns_string};
use winit::event_loop::EventLoopProxy;

use crate::event::{Event, EventType};

struct MenuTargetIvars {
    proxy: EventLoopProxy<Event>,
}

define_class!(
    // SAFETY:
    // - `#[unsafe(super(NSObject))]`: `NSObject` imposes no subclassing
    //   requirements, and `MenuTarget` does not implement `Drop`.
    // - `unsafe impl NSObjectProtocol`: its required methods are all inherited from
    //   the `NSObject` superclass.
    // - `#[unsafe(method(newWindow:/newTab:/openDocumentation:))]`: action selectors, so the
    //   Rust signatures `fn(&self, Option<&AnyObject>)` returning `()` match the
    //   Objective-C `- (void)action:(id)sender` types AppKit invokes them with.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AlacrittyMenuTarget"]
    #[ivars = MenuTargetIvars]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(newWindow:))]
        fn new_window(&self, _sender: Option<&AnyObject>) {
            self.request_window(false);
        }

        #[unsafe(method(newTab:))]
        fn new_tab(&self, _sender: Option<&AnyObject>) {
            self.request_window(true);
        }

        #[unsafe(method(openDocumentation:))]
        fn open_documentation(&self, _sender: Option<&AnyObject>) {
            let docs = ns_string!("https://alacritty.org/config-alacritty.html");
            if let Some(url) = NSURL::URLWithString(docs) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker, proxy: EventLoopProxy<Event>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(MenuTargetIvars { proxy });
        unsafe { msg_send![super(this), init] }
    }

    fn request_window(&self, tabbed: bool) {
        let event = Event::new(EventType::CreateWindowFromMenu { tabbed }, None);
        let _ = self.ivars().proxy.send_event(event);
    }
}

/// Add the File, Window, and Help menus to winit's menu bar.
pub fn initialize(mtm: MainThreadMarker, proxy: EventLoopProxy<Event>) {
    let app = NSApplication::sharedApplication(mtm);
    let main_menu =
        app.mainMenu().expect("winit builds the default menubar unless with_default_menu(false)");

    // Target for our own file menu actions.
    let target = MenuTarget::new(mtm, proxy);
    let target_obj: &AnyObject = &target;

    // File menu: our actions, dispatched to `target`.
    let file_menu = NSMenu::new(mtm);
    for (title, action, key) in [
        (ns_string!("New Window"), sel!(newWindow:), ns_string!("n")),
        (ns_string!("New Tab"), sel!(newTab:), ns_string!("t")),
    ] {
        file_menu.addItem(&action_item(mtm, title, action, key, Some(target_obj)));
    }

    // Close the key window (Cmd+W), routed through the responder chain.
    file_menu.addItem(&action_item(
        mtm,
        ns_string!("Close"),
        sel!(performClose:),
        ns_string!("w"),
        None,
    ));
    main_menu.addItem(&submenu_item(mtm, ns_string!("File"), &file_menu));

    // Window menu: standard AppKit actions on the key window (no target).
    let window_menu = NSMenu::new(mtm);
    for (title, action, key) in [
        (ns_string!("Minimize"), sel!(performMiniaturize:), ns_string!("m")),
        (ns_string!("Zoom"), sel!(performZoom:), ns_string!("")),
    ] {
        window_menu.addItem(&action_item(mtm, title, action, key, None));
    }

    // Enter Full Screen (Ctrl+Cmd+F). AppKit toggles the title to "Exit Full
    // Screen" automatically.
    let fullscreen = action_item(
        mtm,
        ns_string!("Enter Full Screen"),
        sel!(toggleFullScreen:),
        ns_string!("f"),
        None,
    );
    let mask = NSEventModifierFlags::Control | NSEventModifierFlags::Command;
    fullscreen.setKeyEquivalentModifierMask(mask);
    window_menu.addItem(&fullscreen);
    main_menu.addItem(&submenu_item(mtm, ns_string!("Window"), &window_menu));

    // Auto-populate the system window commands and tiling entries.
    app.setWindowsMenu(Some(&window_menu));

    // Help menu: a documentation link, plus the search field AppKit inserts
    // automatically once the menu is designated via `setHelpMenu:`.
    let help_menu = NSMenu::new(mtm);
    help_menu.addItem(&action_item(
        mtm,
        ns_string!("Alacritty Documentation"),
        sel!(openDocumentation:),
        ns_string!(""),
        Some(target_obj),
    ));
    main_menu.addItem(&submenu_item(mtm, ns_string!("Help"), &help_menu));
    app.setHelpMenu(Some(&help_menu));

    // TODO: Move this to an explicit owner?
    std::mem::forget(target);
}

/// Build a menu item for `action`, targeting `target` or falling back to the
/// default responder chain.
fn action_item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: &NSString,
    target: Option<&AnyObject>,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            title,
            Some(action),
            key,
        )
    };
    if let Some(target) = target {
        unsafe { item.setTarget(Some(target)) };
    }
    item
}

/// Build a menu item hosting `submenu`.
fn submenu_item(mtm: MainThreadMarker, title: &NSString, submenu: &NSMenu) -> Retained<NSMenuItem> {
    let item = NSMenuItem::new(mtm);
    item.setTitle(title);
    item.setSubmenu(Some(submenu));
    item
}
