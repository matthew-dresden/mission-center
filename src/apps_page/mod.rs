/* apps_page/mod.rs
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

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::fmt::Write;

use adw::glib::{ParamSpec, Properties, Value};
use adw::prelude::*;
use arrayvec::ArrayString;
use glib::translate::from_glib_full;
use glib::{gobject_ffi, Object};
use gtk::{gio, glib, subclass::prelude::*};

use crate::magpie_client::App;

use crate::i18n::{i18n, ni18n_f};
use crate::process_tree::models::{
    base_model, filter_list_model, sort_list_model, tree_list_model, update_apps, update_processes,
};
use crate::process_tree::row_model::{ContentType, RowModel};

mod imp {
    use super::*;
    use crate::process_tree::column_view_frame::ColumnViewFrame;
    use crate::process_tree::columns::adjust_view_header_alignment;
    use crate::process_tree::process_action_bar::ProcessActionBar;
    use crate::process_tree::row_model::{ContentType, RowModel, RowModelBuilder, SectionType};

    #[derive(Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::AppsPage)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/apps_page/page.ui")]
    pub struct AppsPage {
        #[template_child]
        pub h1: TemplateChild<gtk::Label>,
        #[template_child]
        pub h2: TemplateChild<gtk::Label>,

        #[template_child]
        pub collapse_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub column_view: TemplateChild<ColumnViewFrame>,
        #[template_child]
        pub process_action_bar: TemplateChild<ProcessActionBar>,

        pub apps_section: RowModel,
        pub processes_section: RowModel,

        pub root_process: Cell<u32>,
        pub running_apps: RefCell<HashMap<String, App>>,

        pub row_sorter: OnceCell<gtk::TreeListRowSorter>,

        pub app_icons: RefCell<HashMap<u32, String>>,
        pub selected_item: RefCell<RowModel>,
    }

    impl Default for AppsPage {
        fn default() -> Self {
            Self {
                h1: TemplateChild::default(),
                h2: TemplateChild::default(),
                collapse_label: TemplateChild::default(),
                column_view: TemplateChild::default(),
                process_action_bar: TemplateChild::default(),

                apps_section: RowModelBuilder::new()
                    .name(&i18n("Apps"))
                    .content_type(ContentType::SectionHeader)
                    .section_type(SectionType::FirstSection)
                    .build(),
                processes_section: RowModelBuilder::new()
                    .name(&i18n("Processes"))
                    .content_type(ContentType::SectionHeader)
                    .section_type(SectionType::SecondSection)
                    .build(),

                root_process: Cell::new(1),
                running_apps: RefCell::new(HashMap::new()),

                row_sorter: OnceCell::new(),

                app_icons: RefCell::new(HashMap::new()),
                selected_item: RefCell::new(RowModelBuilder::new().build()),
            }
        }
    }

    impl AppsPage {
        pub fn collapse(&self) {
            self.collapse_label.set_visible(false);

            self.h2.set_visible(false);

            self.process_action_bar.imp().collapse();
        }

        pub fn expand(&self) {
            self.collapse_label.set_visible(true);

            self.h2.set_visible(true);

            self.process_action_bar.imp().expand();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AppsPage {
        const NAME: &'static str = "AppsPage";
        type Type = super::AppsPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            RowModel::ensure_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AppsPage {
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
        }
    }

    impl WidgetImpl for AppsPage {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl BoxImpl for AppsPage {}
}

glib::wrapper! {
    pub struct AppsPage(ObjectSubclass<imp::AppsPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl AppsPage {
    pub fn set_initial_readings(&self, readings: &mut crate::magpie_client::Readings) -> bool {
        let imp = self.imp();

        imp.column_view.imp().setup(
            &imp.apps_section,
            &imp.processes_section,
            Some(&imp.process_action_bar),
            None,
            None,
        );

        self.update_common(readings);

        true
    }

    pub fn update_readings(&self, readings: &mut crate::magpie_client::Readings) -> bool {
        let imp = self.imp();

        self.update_common(readings);

        if let Some(row_sorter) = imp.column_view.imp().row_sorter.get() {
            row_sorter.changed(gtk::SorterChange::Different)
        }

        if readings.network_stats_error.is_some() {
            imp.column_view
                .get()
                .imp()
                .network_usage_column
                .set_visible(false);
        }

        true
    }

    fn update_common(&self, readings: &mut crate::magpie_client::Readings) {
        let imp = self.imp();

        let mut buffer = ArrayString::<64>::new();
        let running_apps_len = readings.running_apps.len() as u32;
        let _ = write!(&mut buffer, "{}", running_apps_len);
        imp.h1.set_label(&ni18n_f(
            "{} Running App",
            "{} Running Apps",
            running_apps_len,
            &[buffer.as_str()],
        ));

        buffer.clear();
        let running_processes_len = readings.running_processes.len() as u32;
        let _ = write!(&mut buffer, "{}", running_processes_len);
        imp.h2.set_label(&ni18n_f(
            "{} Running Process",
            "{} Running Processes",
            running_processes_len,
            &[buffer.as_str()],
        ));

        imp.column_view.imp().update_column_titles(readings);

        let mut process_model_map = HashMap::new();
        let root_process = readings.running_processes.keys().min().unwrap_or(&1);
        if let Some(init) = readings.running_processes.get(root_process) {
            for child in &init.children {
                update_processes(
                    &readings.running_processes,
                    child,
                    &imp.processes_section.children(),
                    &imp.app_icons.borrow(),
                    "application-x-executable-symbolic",
                    imp.column_view.imp().use_merged_stats.get(),
                    &mut process_model_map,
                );
            }
        }
        imp.root_process.set(*root_process);

        update_apps(
            &readings.running_apps,
            &readings.running_processes,
            &process_model_map,
            &mut imp.app_icons.borrow_mut(),
            &imp.apps_section.children(),
        );

        let _ = std::mem::replace(
            &mut *imp.running_apps.borrow_mut(),
            std::mem::take(&mut readings.running_apps),
        );
    }

    #[inline]
    pub fn collapse(&self) {
        self.imp().collapse();
    }

    #[inline]
    pub fn expand(&self) {
        self.imp().expand();
    }

    pub fn running_apps(&self) -> HashMap<String, App> {
        self.imp().running_apps.borrow().clone()
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
