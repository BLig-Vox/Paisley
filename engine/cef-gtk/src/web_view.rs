/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::key_tables::KeyTables;
use crate::proto_ipc::{
    AuthRequest, ContextMenuRequest, FileChooserRequest, PermissionRequest, ScriptDialogKind,
    ScriptDialogRequest, ServoEvent, servo_event,
};
use crate::transport::{LogLevel, PageRunner};
use crate::user_content::UserContentManager;
use glib::info;
use glib::subclass::Signal;
use glib::translate::*;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{glib, subclass::prelude::*};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

const G_LOG_DOMAIN: &str = "ServoGtk";

/// Convert a logical widget coordinate or delta to the device pixels Servo
/// renders and receives input in.
///
/// The scale is the display's *fractional* device-pixel ratio (e.g. 1.25),
/// read from the widget's `GdkSurface`. Servo's surface is sized in device
/// pixels, so pointer and scroll values measured in logical units must be
/// multiplied by the same factor. A non-positive scale is treated as 1.0.
fn logical_to_device(value: f64, scale: f64) -> f64 {
    value * if scale > 0.0 { scale } else { 1.0 }
}

/// The state of a page load, delivered by the [`WebView::connect_load_changed`]
/// signal.
///
/// Servo only surfaces load start and load completion, so only the
/// corresponding two states are represented here. Observe [`WebView::uri`]
/// changes via `notify::uri` for finer-grained navigation events such as
/// redirects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, glib::Enum)]
#[enum_type(name = "ServoGtkLoadEvent")]
pub enum LoadEvent {
    /// A new load request has started.
    #[default]
    Started,
    /// Loading has finished (all resources loaded, `document.readyState` ==
    /// `complete`).
    Finished,
    Committed,
}

mod imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::WebView)]
    pub struct WebView {
        pub servo_runner: RefCell<Option<Rc<PageRunner>>>,
        #[property(get, set, construct_only, nullable)]
        pub config_dir: RefCell<Option<String>>,
        #[property(get, set, construct_only)]
        pub temporary_storage: Cell<bool>,
        #[property(get, set, construct_only)]
        pub inspected_page: Cell<u64>,
        pub stopped: Cell<bool>,
        pub script_dialog: RefCell<Option<(u64, glib::WeakRef<gtk::Window>)>>,
        pub auth_dialog: RefCell<Option<(u64, glib::WeakRef<gtk::Window>)>>,
        pub file_choice: RefCell<Option<(u64, glib::WeakRef<gtk::Window>)>>,
        pub permission_dialog: RefCell<Option<(u64, gio::Cancellable)>>,
        pub page_activity: Cell<Option<(std::time::Instant, bool)>>,
        pub certificate: RefCell<Option<gio::TlsCertificate>>,
        pub certificate_flags: Cell<u32>,
        pub insecure: Cell<bool>,
        pub renderer_generation: Cell<u64>,
        pub blocking: RefCell<crate::proto_ipc::Blocking>,
        #[property(get)]
        pub can_go_back: Cell<bool>,
        #[property(get)]
        pub can_go_forward: Cell<bool>,
        #[property(get, nullable)]
        pub favicon: RefCell<Option<gdk::Texture>>,
        pub memory_texture: RefCell<Option<gdk::MemoryTexture>>,
        pub popup_texture: RefCell<Option<gdk::MemoryTexture>>,
        pub popup_serial: Cell<u64>,
        pub popup_visible: Cell<bool>,
        pub popup_x: Cell<i32>,
        pub popup_y: Cell<i32>,
        #[cfg(feature = "test-support")]
        pub popup_has_content: Cell<bool>,
        pub key_tables: KeyTables,
        /// The user content manager attached to this WebView, if any.
        pub user_content_manager: RefCell<Option<UserContentManager>>,

        /// The URI of the currently loaded page, or `None` if nothing has been
        /// loaded yet.
        #[property(get, name = "uri", nullable)]
        pub uri: RefCell<Option<String>>,
        /// The title of the currently loaded page.
        #[property(get, name = "title", nullable)]
        pub title: RefCell<Option<String>>,
        /// Whether the view is currently loading a page.
        #[property(get, name = "is-loading")]
        pub is_loading: Cell<bool>,
        /// The page zoom level, where 1.0 is unzoomed.
        ///
        /// Mirrored here rather than read back from the runner so the property
        /// is readable and settable at any point, including before the runner
        /// has applied it. The custom setter (see [`super::WebView::set_zoom_level`])
        /// clamps and forwards the value to the runner.
        #[property(get, set = Self::set_zoom_level, name = "zoom-level", minimum = 0.1, maximum = 10.0, default = 1.0)]
        pub zoom_level: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WebView {
        const NAME: &'static str = "ServoGtkWebView";
        type Type = super::WebView;
        type ParentType = gtk::Widget;
    }

    impl WebView {
        /// Property setter for `zoom-level`.
        ///
        /// Clamps the requested level to the range Servo accepts, caches it so
        /// the getter reflects it immediately, and forwards it to the runner.
        /// The runner clamps again defensively.
        fn set_zoom_level(&self, level: f64) {
            let clamped = level.clamp(0.1, 10.0);
            self.zoom_level.set(clamped);
            if let Some(servo) = self.servo_runner.borrow().as_ref() {
                servo.set_zoom_level(clamped);
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for WebView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("renderer-terminated").build(),
                    Signal::builder("page-error")
                        .param_types([String::static_type()])
                        .build(),
                    // Emitted when the load state of the page changes.
                    Signal::builder("load-changed")
                        .param_types([LoadEvent::static_type()])
                        .build(),
                    // Emitted when page content requests a new top-level
                    // WebView (window.open / target=_blank). The single string
                    // parameter is the destination URL. A handler is expected
                    // to open a new window hosting a fresh WebView for it.
                    Signal::builder("create-web-view")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.zoom_level.set(1.0);

            // Event controllers
            let motion_controller = gtk::EventControllerMotion::new();
            // Enter can be the only pointer event before the first wheel event.
            let obj_weak = self.obj().downgrade();
            motion_controller.connect_enter(move |_, x, y| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref() {
                        let scale = obj.device_scale();
                        servo.motion(logical_to_device(x, scale), logical_to_device(y, scale));
                    }
                }
            });
            let obj_weak = self.obj().downgrade();
            motion_controller.connect_motion(move |_, x, y| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref() {
                        let scale = obj.device_scale();
                        servo.motion(logical_to_device(x, scale), logical_to_device(y, scale));
                    }
                }
            });
            self.obj().add_controller(motion_controller);

            let click_count = std::rc::Rc::new(std::cell::Cell::new(1));
            let click = gtk::GestureClick::new();
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            let press_count = click_count.clone();
            click.connect_pressed(move |_, count, _, _| press_count.set(count));
            let release_count = click_count.clone();
            click.connect_released(move |_, count, _, _| release_count.set(count));
            self.obj().add_controller(click);

            let legacy_controller = gtk::EventControllerLegacy::new();
            let obj_weak = self.obj().downgrade();
            legacy_controller.connect_event(move |controller, event| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref()
                        && let Some((x, y)) = obj.translate_event_coordinates(event)
                    {
                        let scale = obj.device_scale();
                        let x = logical_to_device(x, scale);
                        let y = logical_to_device(y, scale);
                        match event.event_type() {
                            gdk::EventType::ButtonPress => {
                                controller.widget().expect("Controller widget").grab_focus();
                                if let Some(button_event) = event.downcast_ref::<gdk::ButtonEvent>()
                                {
                                    servo.button_press(button_event.button(), x, y, click_count.replace(1));
                                }
                            }
                            gdk::EventType::ButtonRelease => {
                                if let Some(button_event) = event.downcast_ref::<gdk::ButtonEvent>()
                                {
                                    servo.button_release(button_event.button(), x, y, click_count.replace(1));
                                }
                            }
                            gdk::EventType::TouchBegin => {
                                click_count.set(1);
                                servo.touch_begin(x, y);
                                controller.widget().expect("Controller widget").grab_focus();
                            }
                            gdk::EventType::TouchUpdate => {
                                servo.touch_update(x, y);
                            }
                            gdk::EventType::TouchEnd => {
                                click_count.set(1);
                                servo.touch_end(x, y);
                            }
                            gdk::EventType::TouchCancel => {
                                servo.touch_cancel(x, y);
                            }
                            _ => {}
                        }
                    }
                }
                glib::Propagation::Proceed
            });
            self.obj().add_controller(legacy_controller);

            let focus_controller = gtk::EventControllerFocus::new();
            let obj_weak = self.obj().downgrade();
            focus_controller.connect_enter(move |_| {
                if let Some(obj) = obj_weak.upgrade() {
                    if let Some(runner) = obj.imp().servo_runner.borrow().as_ref() {
                        runner.set_focused(true);
                    }
                }
            });
            let obj_weak = self.obj().downgrade();
            focus_controller.connect_leave(move |_| {
                if let Some(obj) = obj_weak.upgrade() {
                    if let Some(runner) = obj.imp().servo_runner.borrow().as_ref() {
                        runner.set_focused(false);
                    }
                }
            });
            self.obj().add_controller(focus_controller);

            let key_controller = gtk::EventControllerKey::new();
            let obj_weak = self.obj().downgrade();
            key_controller.connect_key_pressed(move |_, keyval, keycode, state| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref()
                        && let Some((key, is_character, location)) =
                            imp.key_tables.key_from_keyval(keyval.into_glib())
                    {
                        servo.key_press(key, is_character, location, keycode, state.bits());
                    }
                }
                glib::Propagation::Proceed
            });
            let obj_weak = self.obj().downgrade();
            key_controller.connect_key_released(move |_, keyval, keycode, state| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref()
                        && let Some((key, is_character, location)) =
                            imp.key_tables.key_from_keyval(keyval.into_glib())
                    {
                        servo.key_release(key, is_character, location, keycode, state.bits());
                    }
                }
            });
            self.obj().add_controller(key_controller);

            let scroll_controller =
                gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
            let obj_weak = self.obj().downgrade();
            scroll_controller.connect_scroll(move |_, delta_x, delta_y| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if let Some(servo) = imp.servo_runner.borrow().as_ref() {
                        let scale = obj.device_scale();
                        servo.scroll(
                            logical_to_device(delta_x, scale),
                            logical_to_device(delta_y, scale),
                        );
                    }
                }
                glib::Propagation::Stop
            });
            self.obj().add_controller(scroll_controller);

            // When the widget moves to a monitor with a different scale factor,
            // its logical size is unchanged so `size_allocate` does not fire.
            // Re-sync the Servo surface so it renders at the new device
            // resolution and the page sees the right devicePixelRatio.
            //
            // A fractional scale change (e.g. 1.0 -> 1.25) does not change the
            // integer scale-factor, so we watch the widget's scale-factor
            // *and*, once realized, the GdkSurface's fractional `scale`.
            fn resync_surface(obj: &super::WebView) {
                let imp = obj.imp();
                if let Some(servo) = imp.servo_runner.borrow().as_ref() {
                    let scale = obj.device_scale();
                    let width = gtk::prelude::WidgetExt::width(obj) as f64;
                    let height = gtk::prelude::WidgetExt::height(obj) as f64;
                    servo.set_hidpi_scale_factor(scale as f32);
                    servo.resize(
                        (width * scale).round() as u32,
                        (height * scale).round() as u32,
                    );
                }
            }

            self.obj().connect_scale_factor_notify(resync_surface);

            // Watch the fractional surface scale once the widget is realized.
            self.obj().connect_realize(|obj| {
                if let Some(native) = gtk::prelude::WidgetExt::native(obj)
                    && let Some(surface) = native.surface()
                {
                    let obj_weak = obj.downgrade();
                    surface.connect_scale_notify(move |_| {
                        if let Some(obj) = obj_weak.upgrade() {
                            resync_surface(&obj);
                        }
                    });
                    // Apply the true fractional scale now that it is known.
                    resync_surface(obj);
                }
            });

            self.obj().set_focusable(true);
            info!("Webview constructed");
        }

        fn dispose(&self) {
            self.obj().dismiss_dialogs(None);
            if let Some(servo) = self.servo_runner.borrow().as_ref() {
                servo.shutdown();
            }
        }
    }

    impl WidgetImpl for WebView {
        fn map(&self) {
            self.parent_map();
            if let Some(runner) = self.servo_runner.borrow().as_ref() {
                runner.set_visible(true);
            }
        }
        fn unmap(&self) {
            if let Some(runner) = self.servo_runner.borrow().as_ref() {
                runner.set_visible(false);
            }
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(texture) = self.memory_texture.borrow().as_ref() {
                let bounds = gtk::graphene::Rect::new(
                    0.0,
                    0.0,
                    gtk::prelude::WidgetExt::width(self.obj().as_ref()) as f32,
                    gtk::prelude::WidgetExt::height(self.obj().as_ref()) as f32,
                );
                snapshot.append_texture(texture, &bounds);
            }
            if self.popup_visible.get() {
                if let Some(texture) = self.popup_texture.borrow().as_ref() {
                    let scale = self.obj().device_scale() as f32;
                    let clip = gtk::graphene::Rect::new(
                        0.0,
                        0.0,
                        self.obj().width() as f32,
                        self.obj().height() as f32,
                    );
                    let bounds = gtk::graphene::Rect::new(
                        self.popup_x.get() as f32 / scale,
                        self.popup_y.get() as f32 / scale,
                        texture.width() as f32 / scale,
                        texture.height() as f32 / scale,
                    );
                    snapshot.push_clip(&clip);
                    snapshot.append_texture(texture, &bounds);
                    snapshot.pop();
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if let Some(servo) = self.servo_runner.borrow().as_ref() {
                // Servo's surface is sized in device pixels. Multiply the
                // logical allocation by the display's fractional scale and hand
                // that same factor to Servo as the HiDPI scale, so the page is
                // laid out at the right size rather than upscaled from logical
                // size. Using the fractional scale (e.g. 1.25) rather than the
                // integer scale-factor (which rounds 1.25 up to 2) keeps the
                // page from rendering too large on fractionally scaled displays.
                let scale = self.obj().device_scale();
                servo.set_hidpi_scale_factor(scale as f32);
                servo.resize(
                    ((width.max(0) as f64) * scale).round() as u32,
                    ((height.max(0) as f64) * scale).round() as u32,
                );
            }
        }
    }
}

glib::wrapper! {
    pub struct WebView(ObjectSubclass<imp::WebView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[allow(clippy::new_without_default)]
impl WebView {
    pub fn for_profile(config_dir: &str, temporary_storage: bool) -> Self {
        glib::Object::builder()
            .property("config-dir", config_dir)
            .property("temporary-storage", temporary_storage)
            .build()
    }

    pub fn has_painted_frame(&self) -> bool {
        self.imp().memory_texture.borrow().is_some()
    }
    pub fn inspector_for(source: &Self) -> Option<Self> {
        let target = source.page_id()?;
        let config = source.config_dir()?;
        let view: Self = glib::Object::builder()
            .property("config-dir", config)
            .property("temporary-storage", source.temporary_storage())
            .property("inspected-page", target)
            .build();
        view.ensure_runner();
        Some(view)
    }
    fn ensure_runner(&self) {
        let imp = self.imp();
        let stale = imp
            .servo_runner
            .borrow()
            .as_ref()
            .is_some_and(|r| r.is_closed());
        if stale {
            imp.servo_runner.borrow_mut().take();
        }
        if imp.servo_runner.borrow().is_some() || imp.stopped.get() {
            return;
        }
        let config = imp.config_dir.borrow();
        let runner = Rc::new(if imp.inspected_page.get() == 0 {
            PageRunner::new(config.as_deref(), imp.temporary_storage.get())
        } else {
            PageRunner::inspector(
                config.as_deref(),
                imp.temporary_storage.get(),
                imp.inspected_page.get(),
            )
        });
        drop(config);
        let generation = imp.renderer_generation.get().wrapping_add(1);
        imp.renderer_generation.set(generation);
        let events = runner.event_receiver();
        let policy = imp.blocking.borrow();
        runner.set_blocking(policy.enabled, policy.exceptions.clone());
        runner.set_visible(self.is_mapped());
        runner.set_focused(self.has_focus());
        if let Some(manager) = imp.user_content_manager.borrow().as_ref() {
            manager.attach(runner.clone());
        }
        let scale = self.device_scale();
        runner.set_hidpi_scale_factor(scale as f32);
        runner.resize(
            (self.width().max(1) as f64 * scale) as u32,
            (self.height().max(1) as f64 * scale) as u32,
        );
        runner.set_zoom_level(imp.zoom_level.get());
        imp.servo_runner.replace(Some(runner));
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(event) = events.recv().await {
                let Some(view) = weak.upgrade() else {
                    break;
                };
                if view.imp().stopped.get() || view.imp().renderer_generation.get() != generation {
                    break;
                }
                view.process_servo_event(event);
            }
            if let Some(view) = weak.upgrade()
                && !view.imp().stopped.get()
                && view.imp().renderer_generation.get() == generation
            {
                view.shutdown();
                view.set_is_loading(false);
                view.emit_by_name::<()>("renderer-terminated", &[]);
            }
        });
    }

    /// Stop the renderer immediately when its owning tab closes, even if a
    /// callback temporarily retains the GTK widget. Repeated calls are harmless.
    pub fn shutdown(&self) {
        let imp = self.imp();
        imp.stopped.set(true);
        self.dismiss_dialogs(None);
        imp.renderer_generation
            .set(imp.renderer_generation.get().wrapping_add(1));
        if let Some(runner) = imp.servo_runner.take() {
            runner.shutdown();
        }
        imp.memory_texture.take();
        imp.popup_texture.take();
        imp.popup_visible.set(false);
        imp.page_activity.set(None);
        imp.favicon.take();
    }

    #[cfg(feature = "test-support")]
    pub fn test_click(&self, x: f64, y: f64) {
        self.grab_focus();
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            let scale = self.device_scale();
            runner.button_press(1, x * scale, y * scale, 1);
            runner.button_release(1, x * scale, y * scale, 1);
        }
    }
    #[cfg(feature = "test-support")]
    pub fn test_scroll_at(&self, x: f64, y: f64, dy: f64) {
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            let scale = self.device_scale();
            runner.motion(x * scale, y * scale);
            runner.scroll(0.0, dy * scale);
        }
    }
    #[cfg(feature = "test-support")]
    pub fn test_drag(&self, x: f64, y: f64, end_x: f64, end_y: f64) {
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            let scale = self.device_scale();
            runner.motion(x * scale, y * scale);
            runner.button_press(1, x * scale, y * scale, 1);
            runner.motion(end_x * scale, end_y * scale);
            runner.button_release(1, end_x * scale, end_y * scale, 1);
        }
    }
    #[cfg(feature = "test-support")]
    pub fn file_choice_pending(&self) -> bool {
        self.imp().file_choice.borrow().is_some()
    }

    #[cfg(feature = "test-support")]
    pub fn file_choice_window(&self) -> Option<glib::WeakRef<gtk::Window>> {
        self.imp()
            .file_choice
            .borrow()
            .as_ref()
            .map(|(_, window)| window.clone())
    }
    #[cfg(feature = "test-support")]
    pub fn input_surface_ready(&self) -> bool {
        let scale = self.device_scale();
        self.is_mapped()
            && self
                .imp()
                .memory_texture
                .borrow()
                .as_ref()
                .is_some_and(|texture| {
                    texture.width() == (self.width() as f64 * scale) as i32
                        && texture.height() == (self.height() as f64 * scale) as i32
                })
    }
    #[cfg(feature = "test-support")]
    pub fn popup_has_painted_frame(&self) -> bool {
        self.imp().popup_visible.get()
            && self.imp().popup_texture.borrow().is_some()
            && self.imp().popup_has_content.get()
    }
    #[cfg(feature = "test-support")]
    pub fn test_named_key(&self, key: &str, code: u32) {
        self.test_key_with_modifiers(key, code, 0);
    }

    #[cfg(feature = "test-support")]
    pub fn test_key_with_modifiers(&self, key: &str, code: u32, modifiers: u32) {
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            runner.key_press(
                key.into(),
                false,
                crate::key_tables::KeyLocation::Standard,
                code,
                modifiers,
            );
            runner.key_release(
                key.into(),
                false,
                crate::key_tables::KeyLocation::Standard,
                code,
                modifiers,
            );
        }
    }

    pub fn renderer_identifier(&self) -> Option<String> {
        self.imp()
            .servo_runner
            .borrow()
            .as_ref()
            .and_then(|runner| runner.identifier())
    }

    pub fn tls_info(&self) -> Option<(gio::TlsCertificate, gio::TlsCertificateFlags)> {
        Some((
            self.imp().certificate.borrow().clone()?,
            gio::TlsCertificateFlags::from_bits_truncate(self.imp().certificate_flags.get()),
        ))
    }
    pub fn has_insecure_content(&self) -> bool {
        self.imp().insecure.get()
    }
    pub fn page_id(&self) -> Option<u64> {
        self.imp()
            .servo_runner
            .borrow()
            .as_ref()
            .map(|r| r.page_id())
    }
    pub fn page_activity(&self) -> Option<(std::time::Instant, bool)> {
        self.imp().page_activity.get()
    }

    pub fn has_renderer(&self) -> bool {
        self.imp().servo_runner.borrow().is_some()
    }

    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Create a `WebView` with an attached [`UserContentManager`], enabling
    /// user-script/style injection and the page-to-native message channel.
    ///
    /// Analogous to constructing a `WebKitWebView` with a
    /// `WebKitUserContentManager`. Any scripts, style sheets, or message
    /// handlers added to the manager before or after this call take effect on
    /// the next page load.
    pub fn with_user_content_manager(user_content_manager: &UserContentManager) -> Self {
        let web_view: Self = glib::Object::builder().build();
        // Hand the manager a handle to this WebView's runner so it can forward
        // user-content actions directly, then flush anything buffered before
        // attach. The manager depends only on the runner, not on the WebView.
        if let Some(runner) = web_view.imp().servo_runner.borrow().as_ref() {
            user_content_manager.attach(runner.clone());
        }
        web_view
            .imp()
            .user_content_manager
            .replace(Some(user_content_manager.clone()));
        web_view
    }

    /// The [`UserContentManager`] attached to this `WebView`, if any.
    pub fn user_content_manager(&self) -> Option<UserContentManager> {
        self.imp().user_content_manager.borrow().clone()
    }

    /// Evaluate a snippet of JavaScript in the currently loaded page.
    ///
    /// Fire-and-forget: the result of the evaluation is not currently returned
    /// to the caller. Analogous to `webkit_web_view_evaluate_javascript()`.
    pub fn evaluate_javascript(&self, script: &str) {
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.evaluate_javascript(script);
        }
    }

    pub fn load_url(&self, url: &str) {
        self.imp().stopped.set(false);
        self.ensure_runner();
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.load_url(url);
        }
    }

    /// Load an HTML document from an in-memory string, analogous to
    /// `webkit_web_view_load_html()`.
    ///
    /// `base_url` is accepted for API parity and future use; the current
    /// implementation loads the HTML through a `data:` URL, so relative links
    /// are resolved against that data URL rather than `base_url`.
    pub fn load_html(&self, html: &str, base_url: Option<&str>) {
        self.imp().stopped.set(false);
        self.ensure_runner();
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.load_html(html, base_url);
        }
    }

    /// Trigger a request/response round trip over the internal IPC channel.
    ///
    /// Primarily a diagnostic: it proves the bidirectional channel that backs
    /// the delegate features (script dialogs, file chooser, HTTP auth,
    /// permission and context-menu prompts) is wired up. The runner emits a
    /// request which this `WebView` answers automatically; both sides log the
    /// correlation.
    pub fn ping(&self) {
        self.ensure_runner();
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.ping();
        }
    }

    pub fn reload(&self) {
        if self.imp().stopped.replace(false) {
            if let Some(uri) = self.uri() {
                self.load_url(&uri);
            }
        } else if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            runner.reload();
        }
    }

    pub fn stop_loading(&self) {
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            runner.stop_loading();
        }
        self.set_is_loading(false);
    }

    pub fn set_blocking(&self, enabled: bool, exceptions: Vec<String>) {
        self.imp().blocking.replace(crate::proto_ipc::Blocking {
            enabled,
            exceptions: exceptions.clone(),
        });
        if let Some(runner) = self.imp().servo_runner.borrow().as_ref() {
            runner.set_blocking(enabled, exceptions);
        }
    }

    pub fn connect_page_error<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("page-error", false, move |values| {
            f(
                &values[0].get::<Self>().unwrap(),
                &values[1].get::<String>().unwrap(),
            );
            None
        })
    }
    pub fn connect_renderer_terminated<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("renderer-terminated", false, move |values| {
            f(&values[0].get::<WebView>().unwrap());
            None
        })
    }

    pub fn go_back(&self) {
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.go_back();
        }
    }

    pub fn go_forward(&self) {
        let imp = self.imp();
        if let Some(servo) = imp.servo_runner.borrow().as_ref() {
            servo.go_forward();
        }
    }

    /// Connect to the `load-changed` signal, emitted when the load state of the
    /// page changes.
    ///
    /// Note: the read-only `uri`, `title`, and `is-loading` properties each
    /// have a generated getter (`uri()`, `title()`, `is_loading()`) and a
    /// `notify::` signal (`connect_uri_notify`, `connect_title_notify`,
    /// `connect_is_loading_notify`). The read-write `zoom-level` property has
    /// `zoom_level()`/`set_zoom_level()` accessors and `connect_zoom_level_notify`.
    pub fn connect_load_changed<F: Fn(&Self, LoadEvent) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "load-changed",
            false,
            glib::closure_local!(move |obj: &Self, event: LoadEvent| {
                f(obj, event);
            }),
        )
    }

    /// Connect to the `create-web-view` signal, emitted when page content
    /// requests a new top-level web view (`window.open` / `target=_blank`).
    ///
    /// The handler receives the destination URL and is expected to open a new
    /// window hosting a fresh [`WebView`] loading it. Because the engine runs
    /// one view per subprocess, the auxiliary view Servo created is not
    /// rendered here; this signal lets the embedder recreate the popup as a
    /// real, fully functional window.
    pub fn connect_create_web_view<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "create-web-view",
            false,
            glib::closure_local!(move |obj: &Self, url: String| {
                f(obj, &url);
            }),
        )
    }

    /// The display's fractional device-pixel ratio for this widget.
    ///
    /// GTK's integer `scale_factor()` rounds a fractional display scale (e.g.
    /// 1.25) up to the next integer (2), which would make Servo render far too
    /// large. The widget's `GdkSurface` exposes the true fractional scale via
    /// `scale()`; use it when the widget is realized, falling back to the
    /// integer factor (then 1.0) before a surface exists.
    fn device_scale(&self) -> f64 {
        if let Some(native) = gtk::prelude::WidgetExt::native(self)
            && let Some(surface) = native.surface()
        {
            let scale = surface.scale();
            if scale > 0.0 {
                return scale;
            }
        }
        (self.scale_factor().max(1)) as f64
    }

    fn translate_event_coordinates(&self, event: &gdk::Event) -> Option<(f64, f64)> {
        let root = gtk::prelude::WidgetExt::root(self)?;
        let native = root.native()?;
        let (nx, ny) = native.surface_transform();

        let (event_x, event_y) = event.position()?;
        let event_x = event_x - nx;
        let event_y = event_y - ny;

        let point = gtk::graphene::Point::new(event_x as f32, event_y as f32);
        let translated = root.compute_point(self, &point)?;

        Some((translated.x() as f64, translated.y() as f64))
    }

    fn dismiss_dialogs(&self, id: Option<u64>) {
        let permission = {
            let mut pending = self.imp().permission_dialog.borrow_mut();
            if pending.as_ref().is_some_and(|(request, _)| id.is_none_or(|id| id == *request)) {
                pending.take()
            } else { None }
        };
        if let Some((_, cancellable)) = permission { cancellable.cancel(); }
        for slot in [
            &self.imp().script_dialog,
            &self.imp().auth_dialog,
            &self.imp().file_choice,
        ] {
            let current = {
                let mut dialog = slot.borrow_mut();
                if dialog
                    .as_ref()
                    .is_some_and(|(pending, _)| id.is_none_or(|id| id == *pending))
                {
                    dialog.take()
                } else {
                    None
                }
            };
            if let Some(window) = current.and_then(|(_, weak)| weak.upgrade()) {
                window.close();
            }
        }
    }

    /// Dialog callbacks retain weak owners; tab shutdown and engine cancellation
    /// dismiss the window. A stale response cannot reach a replacement runner.
    fn show_script_dialog(&self, request: ScriptDialogRequest) {
        self.dismiss_dialogs(None);
        let kind = ScriptDialogKind::try_from(request.kind).unwrap_or(ScriptDialogKind::Alert);
        let parent = gtk::prelude::WidgetExt::root(self).and_downcast::<gtk::Window>();
        let window = gtk::Window::builder()
            .title("Page message")
            .modal(true)
            .resizable(false)
            .destroy_with_parent(true)
            .build();
        if let Some(parent) = parent.as_ref() {
            window.set_transient_for(Some(parent));
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        let origin = gtk::Label::new(self.uri().as_deref());
        origin.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        origin.set_max_width_chars(60);
        origin.set_xalign(0.0);
        origin.add_css_class("dim-label");
        content.append(&origin);
        let label = gtk::Label::new(Some(&request.message));
        label.set_wrap(true);
        label.set_max_width_chars(60);
        label.set_xalign(0.0);
        content.append(&label);
        let entry = gtk::Entry::new();
        if kind == ScriptDialogKind::Prompt {
            entry.set_text(&request.default_value);
            entry.set_max_length(4096);
            entry.set_activates_default(true);
            content.append(&entry);
        }
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Cancel");
        if kind != ScriptDialogKind::Alert {
            buttons.append(&cancel);
        }
        let ok = gtk::Button::with_label("OK");
        ok.add_css_class("suggested-action");
        buttons.append(&ok);
        content.append(&buttons);
        window.set_child(Some(&content));
        window.set_default_widget(Some(&ok));
        let responded = Rc::new(Cell::new(false));
        let generation = self.imp().renderer_generation.get();
        let owner = self.downgrade();
        let entry = entry.downgrade();
        let respond = Rc::new(move |confirmed: bool| {
            if responded.replace(true) {
                return;
            }
            if let Some(owner) = owner.upgrade() {
                if owner.imp().renderer_generation.get() == generation {
                    let value = entry
                        .upgrade()
                        .map(|entry| entry.text().to_string())
                        .unwrap_or_default();
                    if let Some(runner) = owner.imp().servo_runner.borrow().as_ref() {
                        runner.send_script_dialog_response(request.request_id, confirmed, &value);
                    }
                }
            }
        });
        let weak = window.downgrade();
        let answer = respond.clone();
        ok.connect_clicked(move |_| {
            answer(true);
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        });
        let weak = window.downgrade();
        let answer = respond.clone();
        cancel.connect_clicked(move |_| {
            answer(false);
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        });
        window.connect_close_request(move |_| {
            respond(kind == ScriptDialogKind::Alert);
            glib::Propagation::Proceed
        });
        self.imp()
            .script_dialog
            .replace(Some((request.request_id, window.downgrade())));
        window.present();
    }

    /// Present a native file chooser for a `<input type=file>` element and send
    /// the selected path(s) back to the runner. Cancelling yields an empty
    /// selection.
    // GTK 4.14's portal cancellation clears its connection before the pending
    // OpenFile reply finishes. A local GTK dialog gives this Linux desktop view
    // synchronous owner teardown, without a timer or a dangling portal callback.
    #[allow(deprecated)]
    fn show_file_chooser(&self, request: FileChooserRequest) {
        if self.imp().file_choice.borrow().is_some() {
            return;
        }
        let parent = gtk::prelude::WidgetExt::root(self).and_downcast::<gtk::Window>();
        let action = match request.mode {
            2 => gtk::FileChooserAction::SelectFolder,
            3 => gtk::FileChooserAction::Save,
            _ => gtk::FileChooserAction::Open,
        };
        let title = if request.mode == 3 {
            "Save file"
        } else if request.mode == 2 {
            "Choose folder"
        } else if request.allow_multiple {
            "Open files (up to 16)"
        } else {
            "Open file"
        };
        let dialog = gtk::FileChooserDialog::builder()
            .title(title)
            .action(action)
            .modal(true)
            .destroy_with_parent(true)
            .build();
        if let Some(parent) = parent {
            dialog.set_transient_for(Some(&parent));
        }
        dialog.set_select_multiple(request.allow_multiple);
        dialog.add_buttons(&[
            ("Cancel", gtk::ResponseType::Cancel),
            (
                if request.mode == 3 { "Save" } else { "Open" },
                gtk::ResponseType::Accept,
            ),
        ]);
        dialog.set_default_response(gtk::ResponseType::Accept);
        for pattern in request.filter_patterns.iter().take(32) {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(pattern));
            if pattern.contains('/') {
                filter.add_mime_type(pattern);
            } else if pattern.starts_with('.') {
                filter.add_pattern(&format!("*{pattern}"));
            } else {
                continue;
            }
            dialog.add_filter(&filter);
        }
        if request.mode == 3 {
            if let Some(name) = std::path::Path::new(&request.default_name)
                .file_name()
                .and_then(|s| s.to_str())
            {
                let name: String = name.chars().filter(|c| !c.is_control()).take(255).collect();
                if !name.is_empty() {
                    dialog.set_current_name(&name);
                }
            }
        }
        let request_id = request.request_id;
        self.imp().file_choice.replace(Some((
            request_id,
            dialog.clone().upcast::<gtk::Window>().downgrade(),
        )));
        let generation = self.imp().renderer_generation.get();
        let weak = self.downgrade();
        dialog.connect_response(move |dialog, response| {
            let mut paths = Vec::new();
            if response == gtk::ResponseType::Accept {
                let files = dialog.files();
                if files.n_items() <= 16 {
                    for index in 0..files.n_items() {
                        if let Some(file) = files.item(index).and_downcast::<gio::File>() {
                            if let Some(path) = file.path() {
                                paths.push(path.to_string_lossy().into_owned());
                            }
                        }
                    }
                }
            }
            if let Some(owner) = weak.upgrade() {
                let pending = {
                    let mut slot = owner.imp().file_choice.borrow_mut();
                    if slot.as_ref().is_some_and(|(id, _)| *id == request_id) {
                        slot.take().is_some()
                    } else {
                        false
                    }
                };
                if pending && owner.imp().renderer_generation.get() == generation {
                    if let Some(runner) = owner.imp().servo_runner.borrow().as_ref() {
                        runner.send_file_chooser_response(request_id, paths);
                    }
                }
            }
            dialog.destroy();
        });
        dialog.present();
    }

    /// Present an HTTP authentication prompt (username + password) and reply
    /// with the entered credentials, or a cancellation if dismissed.
    fn show_auth_dialog(&self, request: AuthRequest) {
        let request_id = request.request_id;
        let parent = gtk::prelude::WidgetExt::root(self).and_downcast::<gtk::Window>();
        let window = gtk::Window::builder()
            .title("Authentication Required")
            .modal(true)
            .resizable(false)
            .destroy_with_parent(true)
            .build();
        if let Some(parent) = parent.as_ref() {
            window.set_transient_for(Some(parent));
        }

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        let kind = if request.for_proxy { "proxy" } else { "server" };
        let label = gtk::Label::new(Some(&format!(
            "The {kind} at {} requires a username and password.",
            request.url
        )));
        label.set_wrap(true);
        label.set_xalign(0.0);
        content.append(&label);

        let user_entry = gtk::Entry::builder().placeholder_text("Username").build();
        let pass_entry = gtk::Entry::builder()
            .placeholder_text("Password")
            .visibility(false)
            .activates_default(true)
            .build();
        content.append(&user_entry);
        content.append(&pass_entry);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_halign(gtk::Align::End);
        let cancel_button = gtk::Button::with_label("Cancel");
        let ok_button = gtk::Button::with_label("Authenticate");
        ok_button.add_css_class("suggested-action");
        buttons.append(&cancel_button);
        buttons.append(&ok_button);
        content.append(&buttons);

        window.set_child(Some(&content));

        self.imp()
            .auth_dialog
            .replace(Some((request_id, window.downgrade())));
        user_entry.set_max_length(1024);
        pass_entry.set_max_length(1024);
        let generation = self.imp().renderer_generation.get();
        let responded = Rc::new(Cell::new(false));
        let respond = {
            let obj_weak = self.downgrade();
            let window = window.downgrade();
            let responded = responded.clone();
            move |confirmed: bool, username: String, password: String| {
                if responded.replace(true) {
                    return;
                }
                if let Some(obj) = obj_weak.upgrade()
                    && obj.imp().renderer_generation.get() == generation
                    && let Some(servo) = obj.imp().servo_runner.borrow().as_ref()
                {
                    servo.send_auth_response(request_id, confirmed, &username, &password);
                }
                if let Some(window) = window.upgrade() {
                    window.close();
                }
            }
        };

        let respond_cancel = respond.clone();
        cancel_button.connect_clicked(move |_| respond_cancel(false, String::new(), String::new()));

        let (user_clone, pass_clone) = (user_entry.clone(), pass_entry.clone());
        let respond_ok = respond.clone();
        ok_button.connect_clicked(move |_| {
            respond_ok(
                true,
                user_clone.text().to_string(),
                pass_clone.text().to_string(),
            )
        });

        let respond_close = respond;
        window.connect_close_request(move |_| {
            respond_close(false, String::new(), String::new());
            glib::Propagation::Proceed
        });

        window.present();
    }

    /// Present an allow/deny permission prompt and reply with the decision.
    fn show_permission_dialog(&self, request: PermissionRequest) {
        let request_id = request.request_id;
        let parent = gtk::prelude::WidgetExt::root(self).and_downcast::<gtk::Window>();
        let dialog = gtk::AlertDialog::builder()
            .message(format!(
                "This page is requesting permission to use: {}",
                request.feature_name
            ))
            .modal(true)
            .build();
        dialog.set_buttons(&["Deny", "Allow"]);
        dialog.set_cancel_button(0);
        dialog.set_default_button(0);

        let generation = self.imp().renderer_generation.get();
        let cancellable = gio::Cancellable::new();
        if let Some((_, previous)) = self.imp().permission_dialog.replace(Some((request_id, cancellable.clone()))) {
            previous.cancel();
        }
        let obj_weak = self.downgrade();
        dialog.choose(parent.as_ref(), Some(&cancellable), move |result| {
            let Some(obj) = obj_weak.upgrade() else { return; };
            if obj.imp().stopped.get() || obj.imp().renderer_generation.get() != generation { return; }
            let pending = {
                let mut slot = obj.imp().permission_dialog.borrow_mut();
                if slot.as_ref().is_some_and(|(id, _)| *id == request_id) { slot.take() } else { None }
            };
            if pending.is_some() && let Some(servo) = obj.imp().servo_runner.borrow().as_ref() {
                servo.send_permission_response(request_id, matches!(result, Ok(1)));
            }
        });
    }

    /// Present a native context menu for web content (e.g. on right-click) and
    /// reply with the chosen entry index, or a dismissal.
    ///
    /// Uses a `gtk::Popover` of buttons anchored at the triggering position.
    /// The runner reports the position in device pixels, so it is converted
    /// back to logical coordinates for placement.
    fn show_context_menu(&self, request: ContextMenuRequest) {
        let request_id = request.request_id;
        let popover = gtk::Popover::builder()
            .autohide(true)
            .has_arrow(false)
            .build();
        popover.set_parent(self);

        let scale = self.scale_factor().max(1) as f64;
        let rect = gtk::gdk::Rectangle::new(
            (request.x as f64 / scale) as i32,
            (request.y as f64 / scale) as i32,
            1,
            1,
        );
        popover.set_pointing_to(Some(&rect));

        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        popover.set_child(Some(&menu_box));

        // Single-fire responder: exactly one response per menu, whether a
        // button was clicked or the popover was dismissed.
        let responded = Rc::new(Cell::new(false));
        let respond = {
            let obj_weak = self.downgrade();
            let popover = popover.clone();
            let responded = responded.clone();
            move |selected: bool, index: u32| {
                if responded.replace(true) {
                    return;
                }
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(servo) = obj.imp().servo_runner.borrow().as_ref()
                {
                    servo.send_context_menu_response(request_id, selected, index);
                }
                popover.popdown();
            }
        };

        for (index, entry) in request.items.iter().enumerate() {
            if entry.separator {
                menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                continue;
            }
            let button = gtk::Button::builder()
                .label(&entry.label)
                .sensitive(entry.enabled)
                .has_frame(false)
                .build();
            let respond_item = respond.clone();
            button.connect_clicked(move |_| respond_item(true, index as u32));
            menu_box.append(&button);
        }

        // Closing the popover without a selection is a dismissal.
        let respond_dismiss = respond.clone();
        popover.connect_closed(move |_| respond_dismiss(false, 0));

        // Unparent the popover once it is closed so it does not leak.
        let popover_cleanup = popover.clone();
        popover.connect_closed(move |_| {
            popover_cleanup.unparent();
        });

        popover.popup();
    }

    fn process_servo_event(&self, event: ServoEvent) {
        let Some(event_type) = event.event else {
            return;
        };

        match event_type {
            servo_event::Event::ViewClosed(_) => {
                self.shutdown();
                self.emit_by_name::<()>("renderer-terminated", &[]);
            }
            servo_event::Event::PageError(error) => {
                if error.unavailable {
                    self.shutdown();
                }
                self.set_is_loading(false);
                self.emit_by_name::<()>("page-error", &[&error.message]);
            }
            servo_event::Event::TlsState(state) => {
                let mut flags = gio::TlsCertificateFlags::empty();
                if state.cert_status & 1 != 0 {
                    flags |= gio::TlsCertificateFlags::BAD_IDENTITY;
                }
                if state.cert_status & 2 != 0 {
                    flags |= gio::TlsCertificateFlags::EXPIRED;
                }
                if state.cert_status & 4 != 0 {
                    flags |= gio::TlsCertificateFlags::UNKNOWN_CA;
                }
                if state.cert_status & 64 != 0 {
                    flags |= gio::TlsCertificateFlags::REVOKED;
                }
                if state.certificate_error && flags.is_empty() {
                    flags |= gio::TlsCertificateFlags::GENERIC_ERROR;
                }
                let certificate = std::str::from_utf8(&state.pem)
                    .ok()
                    .and_then(|pem| gio::TlsCertificate::from_pem(pem).ok());
                self.imp().certificate.replace(certificate);
                self.imp().certificate_flags.set(flags.bits());
                self.imp().insecure.set(state.insecure_content);
            }
            servo_event::Event::OperationResult(_) => {}
            servo_event::Event::EngineStats(_)
            | servo_event::Event::DownloadStarted(_)
            | servo_event::Event::DownloadProgress(_) => {}
            servo_event::Event::PageActivity(activity) => {
                self.imp()
                    .page_activity
                    .set(Some((std::time::Instant::now(), activity.protected)));
            }
            servo_event::Event::PopupState(state) => {
                #[cfg(feature = "test-support")]
                if std::env::var_os("PAISLEY_NATIVE_POPUP").is_some() {
                    eprintln!("POPUP ui state {state:?}");
                }
                let imp = self.imp();
                if state.serial >= imp.popup_serial.get() {
                    if !state.visible || state.serial != imp.popup_serial.get() {
                        imp.popup_texture.take();
                    }
                    imp.popup_serial.set(state.serial);
                    imp.popup_visible.set(state.visible);
                    imp.popup_x.set(state.x);
                    imp.popup_y.set(state.y);
                    self.queue_draw();
                }
            }
            servo_event::Event::PopupFrame(popup) => {
                #[cfg(feature = "test-support")]
                if std::env::var_os("PAISLEY_NATIVE_POPUP").is_some() {
                    eprintln!("POPUP ui frame serial {}", popup.serial);
                }
                let imp = self.imp();
                if imp.popup_visible.get() && popup.serial == imp.popup_serial.get() {
                    if let Some(frame) = popup.frame {
                        if !paisley_engine_transport::valid_rgba(
                            frame.width,
                            frame.height,
                            frame.rgba_data.len(),
                        ) {
                            self.shutdown();
                            return;
                        }
                        #[cfg(feature = "test-support")]
                        imp.popup_has_content.set(
                            frame
                                .rgba_data
                                .chunks_exact(4)
                                .any(|pixel| pixel != &frame.rgba_data[..4] && pixel[3] != 0),
                        );
                        let bytes = glib::Bytes::from_owned(frame.rgba_data);
                        let texture = gdk::MemoryTexture::new(
                            frame.width as i32,
                            frame.height as i32,
                            gdk::MemoryFormat::R8g8b8a8Premultiplied,
                            &bytes,
                            frame.width as usize * 4,
                        );
                        imp.popup_texture.replace(Some(texture));
                        self.queue_draw();
                    }
                }
            }
            servo_event::Event::FrameReady(frame_ready) => {
                if !paisley_engine_transport::valid_rgba(
                    frame_ready.width,
                    frame_ready.height,
                    frame_ready.rgba_data.len(),
                ) {
                    self.shutdown();
                    return;
                }
                let imp = self.imp();
                let width = frame_ready.width;
                let height = frame_ready.height;
                // Transfer the received allocation instead of copying every frame.
                let bytes = glib::Bytes::from_owned(frame_ready.rgba_data);
                let texture = gdk::MemoryTexture::new(
                    width as i32,
                    height as i32,
                    gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    width as usize * 4,
                );

                imp.memory_texture.replace(Some(texture));
                self.queue_draw();
            }
            servo_event::Event::HistoryChanged(history) => {
                self.imp().can_go_back.set(history.can_go_back);
                self.imp().can_go_forward.set(history.can_go_forward);
                self.notify("can-go-back");
                self.notify("can-go-forward");
            }
            servo_event::Event::LoadCommitted(load) => {
                if !load.url.is_empty() {
                    self.set_uri(Some(load.url));
                }
                self.emit_load_changed(LoadEvent::Committed);
            }
            servo_event::Event::FaviconChanged(frame) => {
                if paisley_engine_transport::valid_rgba(
                    frame.width,
                    frame.height,
                    frame.rgba_data.len(),
                ) {
                    let texture = gdk::MemoryTexture::new(
                        frame.width as i32,
                        frame.height as i32,
                        gdk::MemoryFormat::R8g8b8a8,
                        &glib::Bytes::from_owned(frame.rgba_data),
                        frame.width as usize * 4,
                    );
                    self.imp().favicon.replace(Some(texture.upcast()));
                    self.notify("favicon");
                }
            }
            servo_event::Event::CursorChanged(cursor_changed) => {
                let gdk_cursor = gdk::Cursor::from_name(&cursor_changed.cursor, None);
                if let Some(cursor) = gdk_cursor {
                    gtk::prelude::WidgetExt::set_cursor(self, Some(&cursor));
                }
            }
            servo_event::Event::UrlChanged(url_changed) => {
                self.set_uri(Some(url_changed.url));
            }
            servo_event::Event::TitleChanged(title_changed) => {
                self.set_title(Some(title_changed.title));
            }
            servo_event::Event::LoadStart(load_start) => {
                if !load_start.url.is_empty() {
                    self.set_uri(Some(load_start.url));
                }
                self.imp().certificate.take();
                self.imp().insecure.set(false);
                self.set_is_loading(true);
                self.emit_load_changed(LoadEvent::Started);
            }
            servo_event::Event::LoadEnd(load_end) => {
                if !load_end.url.is_empty() {
                    self.set_uri(Some(load_end.url));
                }
                self.set_is_loading(false);
                self.emit_load_changed(LoadEvent::Finished);
            }
            servo_event::Event::LogMessage(log_msg) => {
                if let Some(servo_runner) = self.imp().servo_runner.borrow().as_ref() {
                    servo_runner
                        .handle_log_message(LogLevel::from(log_msg.level), &log_msg.message);
                }
            }
            servo_event::Event::ScriptMessage(script_message) => {
                if let Some(ucm) = self.imp().user_content_manager.borrow().as_ref() {
                    ucm.emit_script_message(&script_message.name, &script_message.body);
                }
            }
            servo_event::Event::PingRequest(ping_request) => {
                // Round-trip test of the bidirectional IPC channel: immediately
                // answer the runner's request with the same id. Real delegate
                // features (dialogs, file chooser, ...) show UI here and reply
                // when the user acts.
                info!(
                    "Received PingRequest id {}, replying",
                    ping_request.request_id
                );
                if let Some(servo) = self.imp().servo_runner.borrow().as_ref() {
                    servo.send_ping_response(ping_request.request_id, true);
                }
            }
            servo_event::Event::CancelDialog(id) => self.dismiss_dialogs(Some(id)),
            servo_event::Event::ScriptDialogRequest(request) => {
                self.show_script_dialog(request);
            }
            servo_event::Event::FileChooserRequest(request) => {
                self.show_file_chooser(request);
            }
            servo_event::Event::AuthRequest(request) => {
                self.show_auth_dialog(request);
            }
            servo_event::Event::PermissionRequest(request) => {
                self.show_permission_dialog(request);
            }
            servo_event::Event::CreateWebView(create) => {
                self.emit_by_name::<()>("create-web-view", &[&create.url]);
            }
            servo_event::Event::ContextMenuRequest(request) => {
                self.show_context_menu(request);
            }
        }
    }

    /// Update the cached `uri` and fire `notify::uri` if it changed.
    fn set_uri(&self, uri: Option<String>) {
        let imp = self.imp();
        if *imp.uri.borrow() != uri {
            imp.uri.replace(uri);
            self.notify("uri");
        }
    }

    /// Update the cached `title` and fire `notify::title` if it changed.
    fn set_title(&self, title: Option<String>) {
        let imp = self.imp();
        if *imp.title.borrow() != title {
            imp.title.replace(title);
            self.notify("title");
        }
    }

    /// Update the cached `is-loading` and fire `notify::is-loading` if it
    /// changed.
    fn set_is_loading(&self, is_loading: bool) {
        let imp = self.imp();
        if imp.is_loading.get() != is_loading {
            imp.is_loading.set(is_loading);
            self.notify("is-loading");
        }
    }

    fn emit_load_changed(&self, event: LoadEvent) {
        self.emit_by_name::<()>("load-changed", &[&event]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_event_registers_glib_enum_type() {
        // Registering the enum type must succeed and be stable across calls.
        let ty = LoadEvent::static_type();
        assert_eq!(ty, LoadEvent::static_type());
        assert_eq!(ty.name(), "ServoGtkLoadEvent");
    }

    #[test]
    fn load_event_roundtrips_through_value() {
        for event in [LoadEvent::Started, LoadEvent::Finished] {
            let value = event.to_value();
            let recovered = value.get::<LoadEvent>().expect("value holds a LoadEvent");
            assert_eq!(event, recovered);
        }
    }

    #[test]
    fn logical_to_device_scales_by_factor() {
        assert_eq!(logical_to_device(100.0, 1.0), 100.0);
        assert_eq!(logical_to_device(100.0, 2.0), 200.0);
        assert_eq!(logical_to_device(100.0, 1.25), 125.0);
        assert_eq!(logical_to_device(80.0, 1.5), 120.0);
    }

    #[test]
    fn logical_to_device_treats_non_positive_scale_as_one() {
        assert_eq!(logical_to_device(100.0, 0.0), 100.0);
        assert_eq!(logical_to_device(100.0, -1.0), 100.0);
    }
}
