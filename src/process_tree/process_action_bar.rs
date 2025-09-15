/* process_tree/process_action_bar.rs
 *
 * Copyright 2025 Mission Center Developers
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::glib::translate::from_glib_full;
use adw::glib::{gobject_ffi, Object};
use adw::glib::g_critical;
use adw::prelude::*;
use gtk::{gio, glib, subclass::prelude::*};

use crate::process_tree::row_model::{ContentType, RowModel};
use crate::app;
use crate::process_tree::process_details_dialog::ProcessDetailsDialog;
use crate::process_tree::util::calculate_anchor_point;

macro_rules! setup_action {
    ($this: expr, $action_obj: expr, $magpie_function: ident) => {
        $action_obj.set_enabled(false);
        $action_obj.connect_activate({
            let this = $this.downgrade();
            move |_action, _| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let this = this.imp();

                let selected_item = this.selected_item.borrow();
                // cost savings
                if selected_item.content_type() != ContentType::Process
                    && selected_item.content_type() != ContentType::App
                {
                    return;
                }

                if let Ok(magpie_client) = app!().sys_info() {
                    match selected_item.content_type() {
                        ContentType::Process => {
                            magpie_client.$magpie_function(vec![selected_item.pid()]);
                        }
                        ContentType::App => {
                            magpie_client.$magpie_function(app_pids(&*selected_item));
                        }
                        _ => {}
                    }
                } // log this?
            }
        });
    };
}

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(
        resource = "/io/missioncenter/MissionCenter/ui/process_column_view/process_action_bar.ui"
    )]
    pub struct ProcessActionBar {
        #[template_child]
        pub stop_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub force_stop_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub details_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub context_menu: TemplateChild<gtk::PopoverMenu>,

        pub action_stop: gio::SimpleAction,
        pub action_force_stop: gio::SimpleAction,
        pub action_suspend: gio::SimpleAction,
        pub action_continue: gio::SimpleAction,
        pub action_hangup: gio::SimpleAction,
        pub action_interrupt: gio::SimpleAction,
        pub action_user_one: gio::SimpleAction,
        pub action_user_two: gio::SimpleAction,
        pub action_details: gio::SimpleAction,

        pub action_group: gio::SimpleActionGroup,
    }

    impl Default for ProcessActionBar {
        fn default() -> Self {
            Self {
                stop_label: Default::default(),
                force_stop_label: Default::default(),
                details_label: Default::default(),

                context_menu: Default::default(),

                action_stop: gio::SimpleAction::new("stop", None),
                action_force_stop: gio::SimpleAction::new("force-stop", None),
                action_suspend: gio::SimpleAction::new("suspend", None),
                action_continue: gio::SimpleAction::new("continue", None),
                action_hangup: gio::SimpleAction::new("hangup", None),
                action_interrupt: gio::SimpleAction::new("interrupt", None),
                action_user_one: gio::SimpleAction::new("user-one", None),
                action_user_two: gio::SimpleAction::new("user-two", None),
                action_details: gio::SimpleAction::new("details", None),

                action_group: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProcessActionBar {
        const NAME: &'static str = "ProcessActionBar";
        type Type = super::ProcessActionBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProcessActionBar {
        fn constructed(&self) {
            self.parent_constructed();

            let actions = &self.action_group;
            self.obj().insert_action_group("apps-page", Some(actions));

            actions.add_action(&self.action_stop);
            actions.add_action(&self.action_force_stop);
            actions.add_action(&self.action_suspend);
            actions.add_action(&self.action_continue);
            actions.add_action(&self.action_hangup);
            actions.add_action(&self.action_interrupt);
            actions.add_action(&self.action_user_one);
            actions.add_action(&self.action_user_two);
            actions.add_action(&self.action_details);
            // actions.add_action(self.action_show_context_menu());
        }
    }

    impl WidgetImpl for ProcessActionBar {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl BoxImpl for ProcessActionBar {}

    impl ProcessActionBar {
        pub fn collapse(&self) {
            self.stop_label.set_visible(false);
            self.force_stop_label.set_visible(false);
            self.details_label.set_visible(false);
        }

        pub fn expand(&self) {
            self.stop_label.set_visible(true);
            self.force_stop_label.set_visible(true);
            self.details_label.set_visible(true);
        }

        pub fn configure(
            &self,
            imp: &crate::process_tree::column_view_frame::imp::ColumnViewFrame,
        ) {
            let this = imp.obj();

            imp.action_show_context_menu().connect_activate({
                let this = this.downgrade();
                let slef = self.obj().downgrade();
                move |_action, entry| {
                    let Some(slef) = slef.upgrade() else {
                        return;
                    };

                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let imp = this.imp();

                    let Some(model) = imp.column_view.model() else {
                        g_critical!(
                            "MissionCenter::ProcessActionBar",
                            "Failed to get model for `show-context-menu` action"
                        );
                        return;
                    };

                    let Some((id, anchor_widget, x, y)) =
                        entry.and_then(|s| s.get::<(String, u64, f64, f64)>())
                    else {
                        g_critical!(
                            "MissionCenter::ProcessActionBar",
                            "Failed to get service name and button from show-context-menu action"
                        );
                        return;
                    };

                    let anchor_widget = upgrade_weak_ptr(anchor_widget as _);
                    let (anchor, show_arrow) = calculate_anchor_point(&slef, &anchor_widget, x, y);

                    if select_item(&model, &id) {
                        match imp.selected_item.borrow().content_type() {
                            // should never trigger
                            ContentType::Process | ContentType::App => {
                                slef.imp().context_menu.set_pointing_to(Some(&anchor));
                                slef.imp().context_menu.popup();
                            }
                            _ => {}
                        }
                    }
                }
            });

            setup_action!(this, &self.action_stop, terminate_processes);
            setup_action!(this, &self.action_force_stop, kill_processes);
            setup_action!(this, &self.action_suspend, suspend_processes);
            setup_action!(this, &self.action_continue, continue_processes);
            setup_action!(this, &self.action_hangup, hangup_processes);
            setup_action!(this, &self.action_interrupt, interrupt_processes);
            setup_action!(this, &self.action_user_one, user_signal_one_processes);
            setup_action!(this, &self.action_user_two, user_signal_two_processes);

            (&self.action_details).set_enabled(false);
            (&self.action_details).connect_activate({
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

                    if selected_item.content_type() == ContentType::Process
                        || selected_item.content_type() == ContentType::App
                    {
                        let dialog = ProcessDetailsDialog::new(imp.selected_item.borrow().clone());
                        let self1 = &slef.imp();
                        dialog.insert_action_group("apps-page", Some(&self1.action_group));
                        dialog.present(Some(&this));
                    };
                }
            });
        }

        pub fn handle_changed_selection(&self, row_model: &RowModel) {
            match row_model.content_type() {
                ContentType::Process | ContentType::App => {
                    self.toggle_actions_enabled(true);
                    self.obj().set_visible(true);
                }
                ContentType::SectionHeader => {
                    self.toggle_actions_enabled(false);
                }
                ContentType::Service => {
                    self.toggle_actions_enabled(false);
                    self.obj().set_visible(false);
                }
            }
        }

        fn toggle_actions_enabled(&self, enabled: bool) {
            (&self.action_stop).set_enabled(enabled);
            (&self.action_force_stop).set_enabled(enabled);
            (&self.action_suspend).set_enabled(enabled);
            (&self.action_continue).set_enabled(enabled);
            (&self.action_hangup).set_enabled(enabled);
            (&self.action_interrupt).set_enabled(enabled);
            (&self.action_user_one).set_enabled(enabled);
            (&self.action_user_two).set_enabled(enabled);
            (&self.action_details).set_enabled(enabled);
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
    pub struct ProcessActionBar(ObjectSubclass<imp::ProcessActionBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

fn select_item(model: &gtk::SelectionModel, id: &str) -> bool {
    for i in 0..model.n_items() {
        if let Some(item) = model
            .item(i)
            .and_then(|i| i.downcast::<gtk::TreeListRow>().ok())
            .and_then(|row| row.item())
            .and_then(|obj| obj.downcast::<RowModel>().ok())
        {
            if item.content_type() != ContentType::SectionHeader && item.id() == id {
                model.select_item(i, false);
                return true;
            }
        }
    }

    false
}

fn app_pids(row_model: &RowModel) -> Vec<u32> {
    let children = row_model.children();
    let mut result = Vec::with_capacity(children.n_items() as usize);

    for i in 0..children.n_items() {
        let Some(child) = children
            .item(i)
            .and_then(|i| i.downcast::<RowModel>().ok())
            .and_then(|rm| find_stoppable_child(&rm))
        else {
            continue;
        };
        result.push(child.pid());
    }

    result
}

fn find_stoppable_child(row_model: &RowModel) -> Option<RowModel> {
    if row_model.name() != "bwrap" {
        return Some(row_model.clone());
    }

    let children = row_model.children();
    for i in 0..children.n_items() {
        let Some(child) = children.item(i).and_then(|i| i.downcast::<RowModel>().ok()) else {
            continue;
        };
        if let Some(rm) = find_stoppable_child(&child) {
            return Some(rm);
        }
    }

    None
}
