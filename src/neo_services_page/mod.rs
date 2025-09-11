/* services_page/mod.rs
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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::magpie_client::App;
use adw::prelude::*;
use gtk::Orientation::Horizontal;
use gtk::{gio, glib, subclass::prelude::*};

use crate::i18n::{i18n, i18n_f};
use crate::process_tree::models;
use crate::process_tree::row_model::{ContentType, RowModel, RowModelBuilder, SectionType};
use magpie_types::services::Service;

pub(crate) mod imp {
    use super::*;
    use crate::process_tree::column_view_frame::ColumnViewFrame;
    use crate::process_tree::process_action_bar::ProcessActionBar;
    use crate::process_tree::service_action_bar::ServiceActionBar;
    use gtk::glib::g_critical;
    use gtk::Orientation::{Horizontal, Vertical};

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/services_page/page.ui")]
    pub struct ServicesPage {
        #[template_child]
        pub top_legend: TemplateChild<gtk::Box>,

        #[template_child]
        pub column_view: TemplateChild<ColumnViewFrame>,

        #[template_child]
        pub service_legend: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub total_service_box: TemplateChild<adw::Toggle>,
        #[template_child]
        pub running_service_box: TemplateChild<adw::Toggle>,
        #[template_child]
        pub failed_service_box: TemplateChild<adw::Toggle>,
        #[template_child]
        pub stopped_service_box: TemplateChild<adw::Toggle>,
        #[template_child]
        pub disabled_service_box: TemplateChild<adw::Toggle>,

        #[template_child]
        pub collapse_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub process_action_bar: TemplateChild<ProcessActionBar>,
        #[template_child]
        pub service_action_bar: TemplateChild<ServiceActionBar>,

        pub user_section: RowModel,
        pub system_section: RowModel,

        pub running_apps: RefCell<HashMap<String, App>>,

        pub app_icons: RefCell<HashMap<u32, String>>,

        pub use_merged_stats: Cell<bool>,

        pub action_collapse_all: gio::SimpleAction,
    }

    impl Default for ServicesPage {
        fn default() -> Self {
            Self {
                top_legend: TemplateChild::default(),
                column_view: Default::default(),
                service_legend: TemplateChild::default(),
                total_service_box: TemplateChild::default(),
                running_service_box: TemplateChild::default(),
                failed_service_box: TemplateChild::default(),
                stopped_service_box: TemplateChild::default(),
                disabled_service_box: TemplateChild::default(),

                collapse_label: TemplateChild::default(),

                process_action_bar: Default::default(),
                service_action_bar: Default::default(),

                user_section: RowModelBuilder::new()
                    .name(&i18n("User Services"))
                    .content_type(ContentType::SectionHeader)
                    .section_type(SectionType::FirstSection)
                    .build(),
                system_section: RowModelBuilder::new()
                    .name(&i18n("System Services"))
                    .content_type(ContentType::SectionHeader)
                    .section_type(SectionType::SecondSection)
                    .build(),

                running_apps: RefCell::new(HashMap::new()),

                app_icons: RefCell::new(HashMap::new()),

                use_merged_stats: Cell::new(false),

                action_collapse_all: gio::SimpleAction::new("collapse-all", None),
            }
        }
    }

    impl ServicesPage {
        pub fn collapse(&self) {
            self.collapse_label.set_visible(false);

            self.top_legend.set_orientation(Vertical);

            self.process_action_bar.imp().collapse();
            self.service_action_bar.imp().collapse();
        }

        pub fn expand(&self) {
            self.collapse_label.set_visible(true);

            self.top_legend.set_orientation(Horizontal);

            self.process_action_bar.imp().expand();
            self.service_action_bar.imp().expand();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServicesPage {
        const NAME: &'static str = "ServicesPage";
        type Type = super::ServicesPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            RowModel::ensure_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ServicesPage {
        fn constructed(&self) {
            self.parent_constructed();

            let actions = gio::SimpleActionGroup::new();
            actions.add_action(&self.action_collapse_all);

            self.action_collapse_all.connect_activate({
                let this = self.obj().downgrade();
                move |_action, _| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let imp = this.imp();

                    let Some(selection_model) = imp
                        .column_view
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

                    let mut count = 0;
                    for i in 0..selection_model.n_items() {
                        let Some(row) = selection_model
                            .item(i)
                            .and_then(|item| item.downcast::<gtk::TreeListRow>().ok())
                        else {
                            return;
                        };

                        let Some(row_model) =
                            row.item().and_then(|item| item.downcast::<RowModel>().ok())
                        else {
                            continue;
                        };

                        if row_model.content_type() != ContentType::SectionHeader {
                            continue;
                        }

                        row.set_expanded(false);
                        count += 1;

                        if count >= 2 {
                            break;
                        }
                    }
                }
            });

            self.obj().insert_action_group("apps-page", Some(&actions));
        }
    }

    impl WidgetImpl for ServicesPage {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl BoxImpl for ServicesPage {}
}

glib::wrapper! {
    pub struct ServicesPage(ObjectSubclass<imp::ServicesPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ServicesPage {
    pub fn set_initial_readings(&self, readings: &mut crate::magpie_client::Readings) -> bool {
        let imp = self.imp();

        // Set up the models here since we need access to the main application window
        // which is not yet available in the constructor.
        imp.column_view.imp().setup(
            &imp.user_section,
            &imp.system_section,
            Some(&imp.process_action_bar),
            Some(&imp.service_action_bar),
            Some(&imp.service_legend),
        );

        // Select the first item in the list
        // selection_model.set_selected(0);

        /*        let selected = self.imp().column_view.imp().column_view.selected();
        if selected != INVALID_LIST_POSITION {
            let selected_item = self.imp().column_view.imp().column_view
                .selected_item()
                .and_then(|i| i.downcast_ref::<RowModel>().cloned());

            if let Some(selected_item) = selected_item.as_ref() {
                imp.process_action_bar.imp().handle_changed_selection(selected_item);
                imp.service_action_bar.imp().handle_changed_selection(selected_item);
            }
        }*/

        self.update_common(readings);

        true
    }

    fn update_common(&self, readings: &mut crate::magpie_client::Readings) {
        let imp = self.imp();

        models::update_services(
            &readings.running_processes,
            &readings.services,
            &imp.system_section.children(),
            &imp.app_icons.borrow(),
            "application-x-executable-symbolic",
            imp.column_view.imp().use_merged_stats.get(),
            SectionType::SecondSection,
        );

        models::update_services(
            &readings.running_processes,
            &readings.services,
            &imp.user_section.children(),
            &imp.app_icons.borrow(),
            "application-x-executable-symbolic",
            imp.column_view.imp().use_merged_stats.get(),
            SectionType::FirstSection,
        );

        self.update_section_labels(&readings.services);

        let _ = std::mem::replace(
            &mut *imp.running_apps.borrow_mut(),
            std::mem::take(&mut readings.running_apps),
        );

        let selected_item = &imp.column_view.imp().selected_item.borrow();

        imp.process_action_bar
            .imp()
            .handle_changed_selection(selected_item);
        imp.service_action_bar
            .imp()
            .handle_changed_selection(selected_item);
    }

    fn update_section_labels(&self, services: &HashMap<String, Service>) {
        let services = services.values().collect::<Vec<_>>();

        let total_services = services.len();
        let mut disabled_services = 0;
        let mut running_services = 0;
        let mut stopped_services = 0;
        let mut failed_services = 0;
        for service in services {
            if service.running {
                running_services += 1;
            } else if service.failed {
                failed_services += 1;
            } else if service.enabled {
                stopped_services += 1;
            } else {
                disabled_services += 1;
            }
        }

        let total_string = total_services.to_string();
        let running_string = running_services.to_string();
        let stopped_string = stopped_services.to_string();
        let failed_string = failed_services.to_string();
        let disabled_string = disabled_services.to_string();

        let imp = self.imp();

        let (total_string, running_string, stopped_string, failed_string, disabled_string) =
            // collapsed check
            if imp.top_legend.orientation() == Horizontal {
                (
                    i18n_f("{} Total", &[&total_string]),
                    i18n_f("{} Running", &[&running_string]),
                    i18n_f("{} Stopped", &[&stopped_string]),
                    i18n_f("{} Failed", &[&failed_string]),
                    i18n_f("{} Disabled", &[&disabled_string]),
                )
            } else {
                (
                    total_string,
                    running_string,
                    stopped_string,
                    failed_string,
                    disabled_string,
                )
            };

        imp.total_service_box.set_label(Some(&total_string));
        imp.running_service_box.set_label(Some(&running_string));
        imp.stopped_service_box.set_label(Some(&stopped_string));
        imp.failed_service_box.set_label(Some(&failed_string));
        imp.disabled_service_box.set_label(Some(&disabled_string));
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
