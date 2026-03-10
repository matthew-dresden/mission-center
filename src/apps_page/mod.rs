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

use adw::glib::g_critical;
use adw::prelude::*;
use arrayvec::ArrayString;
use gtk::{gio, glib, subclass::prelude::*};

use crate::i18n::{i18n, ni18n_f};
use crate::settings;
use crate::magpie_client::{App, Process};
use crate::table_view::cached_icon::LightCachedIcon;
use crate::table_view::{
    update_apps, update_apps_flat, update_processes, ContentType, ProcessActionBar, RowModel, RowModelBuilder,
    SectionType, SettingsNamespace, TableView,
};

pub mod actions;

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/apps_page/page.ui")]
    pub struct AppsPage {
        #[template_child]
        pub h1: TemplateChild<gtk::Label>,
        #[template_child]
        pub h2: TemplateChild<gtk::Label>,

        #[template_child]
        pub flat_toggle: TemplateChild<gtk::Button>,
        #[template_child]
        pub flat_toggle_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub flat_toggle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub collapse_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub collapse_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub collapse_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub table_view: TemplateChild<TableView>,
        #[template_child]
        pub process_action_bar: TemplateChild<ProcessActionBar>,

        pub apps_section: RowModel,
        pub processes_section: RowModel,

        pub root_process: Cell<u32>,
        pub running_apps: RefCell<HashMap<String, App>>,
        pub running_processes: RefCell<HashMap<u32, Process>>,

        pub row_sorter: OnceCell<gtk::TreeListRowSorter>,

        pub app_icons: RefCell<HashMap<u32, LightCachedIcon>>,
        pub selected_item: RefCell<RowModel>,
    }

    impl Default for AppsPage {
        fn default() -> Self {
            Self {
                h1: TemplateChild::default(),
                h2: TemplateChild::default(),
                flat_toggle: TemplateChild::default(),
                flat_toggle_icon: TemplateChild::default(),
                flat_toggle_label: TemplateChild::default(),
                collapse_button: TemplateChild::default(),
                collapse_label: TemplateChild::default(),
                collapse_icon: TemplateChild::default(),
                table_view: TemplateChild::default(),
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
                running_processes: RefCell::new(HashMap::new()),

                row_sorter: OnceCell::new(),

                app_icons: RefCell::new(HashMap::new()),
                selected_item: RefCell::new(RowModelBuilder::new().build()),
            }
        }
    }

    impl AppsPage {
        pub fn collapse(&self) {
            self.collapse_label.set_visible(false);
            self.flat_toggle_label.set_visible(false);

            self.h2.set_visible(false);

            self.process_action_bar.imp().collapse();
        }

        pub fn expand(&self) {
            self.collapse_label.set_visible(true);
            self.flat_toggle_label.set_visible(true);

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
        fn constructed(&self) {
            self.parent_constructed();

            let page_actions = gio::SimpleActionGroup::new();

            let action_collapse_all = gio::SimpleAction::new("collapse-all", None);
            action_collapse_all.connect_activate({
                let this = self.obj().downgrade();
                move |_action, _| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let imp = this.imp();

                    let Some(selection_model) = imp
                        .table_view
                        .imp()
                        .column_view
                        .model()
                        .and_then(|model| model.downcast::<gtk::SingleSelection>().ok())
                    else {
                        g_critical!(
                            "MissionCenter::AppsPage",
                            "Failed to get model for `collapse-all` action"
                        );
                        return;
                    };

                    // Collect section headers first — expanding a row mutates the
                    // model, which shifts indices and can cause us to miss rows.
                    let mut section_rows = Vec::with_capacity(2);
                    for i in 0..selection_model.n_items() {
                        let Some(row) = selection_model
                            .item(i)
                            .and_then(|item| item.downcast::<gtk::TreeListRow>().ok())
                        else {
                            continue;
                        };

                        let Some(row_model) =
                            row.item().and_then(|item| item.downcast::<RowModel>().ok())
                        else {
                            continue;
                        };

                        if row_model.content_type() == ContentType::SectionHeader {
                            section_rows.push(row);
                            if section_rows.len() >= 2 {
                                break;
                            }
                        }
                    }

                    // Determine action from actual row state: if any header is
                    // expanded we collapse all, otherwise we expand all.
                    let any_expanded = section_rows.iter().any(|r| r.is_expanded());
                    let target_expanded = !any_expanded;

                    for row in &section_rows {
                        row.set_expanded(target_expanded);
                    }

                    if any_expanded {
                        // We just collapsed everything
                        imp.collapse_label.set_label(&i18n("Expand All"));
                        imp.collapse_icon.set_icon_name(Some("list-expand-symbolic"));
                    } else {
                        // We just expanded everything
                        imp.collapse_label.set_label(&i18n("Collapse All"));
                        imp.collapse_icon.set_icon_name(Some("list-collapse-symbolic"));
                    }
                }
            });

            page_actions.add_action(&action_collapse_all);
            self.obj()
                .insert_action_group("apps-page", Some(&page_actions));

            // Wire up the flat/tree toggle button to GSettings.
            // We listen to the GSettings key so that both the button click
            // and the Alt+F shortcut (which writes the key directly in
            // window.rs) go through the same code path.
            let settings = settings!();
            let flat_mode = settings.boolean("apps-page-flat-process-list");
            self.flat_toggle_icon.set_icon_name(Some(if flat_mode {
                "view-tree-symbolic"
            } else {
                "view-flat-symbolic"
            }));
            self.flat_toggle_label.set_label(&i18n(if flat_mode {
                "Tree View"
            } else {
                "Flat View"
            }));

            // Button click → toggle setting (the connect_changed handler does the rest)
            self.flat_toggle.connect_clicked({
                move |_button| {
                    let settings = settings!();
                    let current = settings.boolean("apps-page-flat-process-list");
                    let _ = settings.set_boolean("apps-page-flat-process-list", !current);
                }
            });

            // Setting changed (from button OR Alt+F shortcut) → update UI + rebuild
            settings.connect_changed(Some("apps-page-flat-process-list"), {
                let this = self.obj().downgrade();
                move |settings, _| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let imp = this.imp();
                    let is_flat = settings.boolean("apps-page-flat-process-list");

                    imp.flat_toggle_icon.set_icon_name(Some(if is_flat {
                        "view-tree-symbolic"
                    } else {
                        "view-flat-symbolic"
                    }));
                    imp.flat_toggle_label.set_label(&i18n(if is_flat {
                        "Tree View"
                    } else {
                        "Flat View"
                    }));

                    this.rebuild_view();
                }
            });

            let process_actions = gio::SimpleActionGroup::new();
            process_actions.add_action(&actions::action_stop(&self.table_view));
            process_actions.add_action(&actions::action_force_stop(&self.table_view));
            process_actions.add_action(&actions::action_suspend(&self.table_view));
            process_actions.add_action(&actions::action_continue(&self.table_view));
            process_actions.add_action(&actions::action_hangup(&self.table_view));
            process_actions.add_action(&actions::action_interrupt(&self.table_view));
            process_actions.add_action(&actions::action_user_one(&self.table_view));
            process_actions.add_action(&actions::action_user_two(&self.table_view));
            process_actions.add_action(&actions::action_details(&self.table_view));
            self.obj()
                .insert_action_group("process", Some(&process_actions));

            self.table_view.imp().setup(
                SettingsNamespace::AppsPage,
                &self.apps_section,
                &self.processes_section,
                Some(&self.process_action_bar),
                None,
                None::<[_; 0]>,
            );
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
        @implements gio::ActionGroup, gio::ActionMap, gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl AppsPage {
    pub fn set_initial_readings(&self, readings: &mut crate::magpie_client::Readings) -> bool {
        self.update_common(readings);

        true
    }

    pub fn update_readings(&self, readings: &mut crate::magpie_client::Readings) -> bool {
        let imp = self.imp();

        self.update_common(readings);

        if let Some(row_sorter) = imp.table_view.imp().row_sorter.get() {
            row_sorter.changed(gtk::SorterChange::Different)
        }

        if readings.network_stats_error.is_some() {
            imp.table_view
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

        imp.table_view.imp().update_column_titles(readings);

        let root_process = readings.running_processes.keys().min().unwrap_or(&1);
        imp.root_process.set(*root_process);

        let flat_mode = settings!().boolean("apps-page-flat-process-list");
        Self::rebuild_view_inner(
            imp,
            flat_mode,
            &readings.running_apps,
            &readings.running_processes,
            *root_process,
        );
        imp.table_view.invalidate_filter();

        let _ = std::mem::replace(
            &mut *imp.running_apps.borrow_mut(),
            std::mem::take(&mut readings.running_apps),
        );
        // Clone instead of take so the services page still has
        // access to running_processes when it runs after us.
        *imp.running_processes.borrow_mut() = readings.running_processes.clone();
    }

    /// Rebuild the flat/tree view using cached data (for instant toggle).
    fn rebuild_view(&self) {
        let imp = self.imp();
        let flat_mode = settings!().boolean("apps-page-flat-process-list");
        let root_process = imp.root_process.get();
        let apps = imp.running_apps.borrow();
        let procs = imp.running_processes.borrow();

        // Clear section children so rebuild starts fresh on a mode toggle.
        imp.apps_section.children().remove_all();
        imp.processes_section.children().remove_all();

        Self::rebuild_view_inner(imp, flat_mode, &apps, &procs, root_process);
    }

    fn rebuild_view_inner(
        imp: &imp::AppsPage,
        flat_mode: bool,
        running_apps: &HashMap<String, App>,
        running_processes: &HashMap<u32, Process>,
        root_process: u32,
    ) {
        if flat_mode {
            // Flat mode: keep Apps/Processes section headers but flatten
            // the contents within each section (no tree hierarchy).
            imp.apps_section.set_name(i18n("Apps").as_str());
            imp.processes_section.set_name(i18n("Processes").as_str());
            update_apps_flat(
                running_apps,
                running_processes,
                &mut imp.app_icons.borrow_mut(),
                &imp.apps_section.children(),
                &imp.processes_section.children(),
                root_process,
            );
        } else {
            imp.apps_section.set_name(i18n("Apps").as_str());
            imp.processes_section.set_name(i18n("Processes").as_str());
            let mut process_model_map = HashMap::new();
            if let Some(init) = running_processes.get(&root_process) {
                update_processes(
                    running_processes,
                    init.children.clone().drain(..).collect(),
                    &imp.processes_section.children(),
                    &imp.app_icons.borrow(),
                    &Default::default(),
                    imp.table_view.imp().use_merged_stats.get(),
                    SectionType::SecondSection,
                    None,
                    &mut process_model_map,
                );
            }

            update_apps(
                running_apps,
                running_processes,
                &process_model_map,
                &mut imp.app_icons.borrow_mut(),
                &imp.apps_section.children(),
            );
        }
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

    pub fn activate_table_view_action(&self, name: &str) -> Result<(), glib::error::BoolError> {
        WidgetExt::activate_action(&*self.imp().table_view, name, None)
    }
}
