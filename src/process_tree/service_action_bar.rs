use std::fmt::Write;

use adw::glib::translate::from_glib_full;
use adw::glib::{gobject_ffi, Object, ParamSpec, Properties, Value};
use adw::prelude::*;
use gtk::{gio, glib, subclass::prelude::*};

use crate::process_tree::row_model::{ContentType, RowModel};

mod imp {
    use super::*;
    use crate::app;
    use crate::magpie_client::MagpieClient;
    use crate::process_tree::column_view_frame::ColumnViewFrame;
    use crate::process_tree::process_details_dialog::ProcessDetailsDialog;
    use crate::process_tree::service_details_dialog::ServiceDetailsDialog;
    use adw::glib::{g_critical, VariantTy};
    use gtk::glib::WeakRef;
    use std::cell::Cell;

    fn find_selected_item(this: WeakRef<ColumnViewFrame>) -> Option<(ColumnViewFrame, RowModel)> {
        let this_obj = match this.upgrade() {
            Some(this) => this,
            None => {
                g_critical!(
                    "MissionCenter::ServicesPage",
                    "Failed to get ServicesPage instance for action"
                );
                return None;
            }
        };
        let this = this_obj.imp();

        let selected_item = this.selected_item.borrow().clone();

        Some((this_obj, selected_item))
    }

    #[derive(Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::ServiceActionBar)]
    #[template(
        resource = "/io/missioncenter/MissionCenter/ui/process_column_view/service_action_bar.ui"
    )]
    pub struct ServiceActionBar {
        #[template_child]
        pub service_start_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub service_stop_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub service_restart_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub service_details_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub service_context_menu: TemplateChild<gtk::PopoverMenu>,

        pub service_start: Cell<gio::SimpleAction>,
        pub service_stop: Cell<gio::SimpleAction>,
        pub service_restart: Cell<gio::SimpleAction>,
        pub service_details: Cell<gio::SimpleAction>,

        pub service_ation_group: Cell<gio::SimpleActionGroup>,
    }

    impl Default for ServiceActionBar {
        fn default() -> Self {
            Self {
                service_start_label: Default::default(),
                service_stop_label: Default::default(),
                service_restart_label: Default::default(),
                service_details_label: Default::default(),
                service_context_menu: Default::default(),

                service_start: Cell::new(gio::SimpleAction::new("selected-svc-start", None)),
                service_stop: Cell::new(gio::SimpleAction::new("selected-svc-stop", None)),
                service_restart: Cell::new(gio::SimpleAction::new("selected-svc-restart", None)),
                service_details: Cell::new(gio::SimpleAction::new("details", None)),

                service_ation_group: Cell::new(Default::default()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServiceActionBar {
        const NAME: &'static str = "ServiceActionBar";
        type Type = super::ServiceActionBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ServiceActionBar {
        fn properties() -> &'static [ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &Value, pspec: &ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }

        fn property(&self, id: usize, pspec: &ParamSpec) -> Value {
            self.derived_property(id, pspec)
        }

        fn constructed(&self) {
            self.parent_constructed();

            let actions = self.service_ation_group();
            self.obj()
                .insert_action_group("services-page", Some(actions));

            actions.add_action(self.service_start());
            actions.add_action(self.service_stop());
            actions.add_action(self.service_restart());
            actions.add_action(self.service_details());
        }
    }

    impl WidgetImpl for ServiceActionBar {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl BoxImpl for ServiceActionBar {}

    impl ServiceActionBar {
        pub fn service_start(&self) -> &gio::SimpleAction {
            unsafe { &*self.service_start.as_ptr() }
        }

        pub fn service_stop(&self) -> &gio::SimpleAction {
            unsafe { &*self.service_stop.as_ptr() }
        }

        pub fn service_restart(&self) -> &gio::SimpleAction {
            unsafe { &*self.service_restart.as_ptr() }
        }

        pub fn service_details(&self) -> &gio::SimpleAction {
            unsafe { &*self.service_details.as_ptr() }
        }

        pub fn service_ation_group(&self) -> &gio::SimpleActionGroup {
            unsafe { &*self.service_ation_group.as_ptr() }
        }

        pub fn configure(
            &self,
            imp: &crate::process_tree::column_view_frame::imp::ColumnViewFrame,
        ) {
            let this = imp.obj();

            self.service_details().set_enabled(false);
            self.service_details().connect_activate({
                let this = this.downgrade();
                let slef = self.obj().downgrade();
                move |_action, _| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let Some(slef) = slef.upgrade() else {
                        return;
                    };
                    let imp = this.imp();

                    let selected_item = imp.selected_item.borrow();

                    if selected_item.content_type() == ContentType::Service {
                        let dialog = ServiceDetailsDialog::new(imp.selected_item.borrow().clone());
                        dialog.insert_action_group(
                            "services-page",
                            Some(slef.imp().service_ation_group()),
                        );
                        dialog.present(Some(&this));
                    };
                }
            });

            fn make_magpie_request(
                this: WeakRef<ColumnViewFrame>,
                request: fn(&MagpieClient, &str),
            ) {
                let app = app!();

                let (_, selected_item) = match find_selected_item(this) {
                    Some((this, item)) => (this, item),
                    None => {
                        g_critical!(
                            "MissionCenter::ServicesPage",
                            "Failed to get selected item for action"
                        );
                        return;
                    }
                };

                match app.sys_info() {
                    Ok(sys_info) => {
                        request(&sys_info, &selected_item.name());
                    }
                    Err(e) => {
                        g_critical!(
                            "MissionCenter::ServicesPage",
                            "Failed to get sys_info from MissionCenterApplication: {}",
                            e
                        );
                    }
                };
            }

            self.service_start().connect_activate({
                let this = imp.obj().downgrade();
                move |_action, _| {
                    make_magpie_request(this.clone(), |sys_info, service_name| {
                        sys_info.start_service(service_name.to_owned());
                    });
                }
            });

            self.service_stop().connect_activate({
                let this = imp.obj().downgrade();
                move |_action, _| {
                    make_magpie_request(this.clone(), |sys_info, service_name| {
                        sys_info.stop_service(service_name.to_owned());
                    });
                }
            });

            self.service_restart().connect_activate({
                let this = imp.obj().downgrade();
                move |_action, _| {
                    make_magpie_request(this.clone(), |sys_info, service_name| {
                        sys_info.restart_service(service_name.to_owned());
                    });
                }
            });
        }

        pub fn handle_changed_selection(&self, row_model: &RowModel) {
            match row_model.content_type() {
                ContentType::Service => {
                    self.obj().set_visible(true);
                    if row_model.service_running() {
                        self.service_stop().set_enabled(true);
                        self.service_start().set_enabled(false);
                        self.service_restart().set_enabled(true);
                    } else {
                        self.service_stop().set_enabled(false);
                        self.service_start().set_enabled(true);
                        self.service_restart().set_enabled(false);
                    }

                    self.service_details().set_enabled(true);
                }
                _ => {
                    self.obj().set_visible(false);
                    self.service_details().set_enabled(false);
                }
            }
        }
    }
}

fn upgrade_weak_ptr(ptr: usize) -> Option<gtk::Widget> {
    let ptr = unsafe { gobject_ffi::g_weak_ref_get(ptr as *mut _) };
    if ptr.is_null() {
        return None;
    }
    let obj: Object = unsafe { from_glib_full(ptr) };
    obj.downcast::<gtk::Widget>().ok()
}

glib::wrapper! {
    pub struct ServiceActionBar(ObjectSubclass<imp::ServiceActionBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}
