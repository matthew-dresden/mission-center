use std::cell::RefCell;
use std::fmt::Write;

use adw::glib::{ParamSpec, Properties, Value};
use adw::prelude::*;
use gtk::{gio, glib, subclass::prelude::*};

use crate::process_tree::columns::*;
use crate::process_tree::row_model::{ContentType, RowModel};

pub(crate) mod imp {
    use super::*;
    use crate::i18n::i18n;
    use crate::process_tree::process_action_bar::ProcessActionBar;
    use crate::process_tree::row_model::RowModelBuilder;
    use crate::process_tree::service_action_bar::ServiceActionBar;
    use crate::process_tree::settings::configure_column_frame;
    use crate::{app, settings, DataType};
    use adw::glib::g_critical;
    use adw::ToggleGroup;
    use arrayvec::ArrayString;
    use gtk::glib::{VariantTy, WeakRef};
    use std::cell::{Cell, OnceCell};
    use textdistance::{Algorithm, Levenshtein};

    #[derive(Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::ColumnViewFrame)]
    #[template(
        resource = "/io/missioncenter/MissionCenter/ui/process_column_view/column_view_frame.ui"
    )]
    pub struct ColumnViewFrame {
        #[template_child]
        pub column_view: TemplateChild<gtk::ColumnView>,
        #[template_child]
        pub name_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub pid_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub cpu_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub memory_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub shared_memory_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub drive_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub network_usage_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub gpu_usage_column: TemplateChild<gtk::ColumnViewColumn>,
        #[template_child]
        pub gpu_memory_column: TemplateChild<gtk::ColumnViewColumn>,

        pub selected_item: RefCell<RowModel>,
        pub row_sorter: OnceCell<gtk::TreeListRowSorter>,

        pub use_merged_stats: Cell<bool>,

        #[property(get, set)]
        pub show_column_separators: Cell<bool>,

        pub action_show_context_menu: Cell<gio::SimpleAction>,

        pub action_group: Cell<gio::SimpleActionGroup>,
    }

    impl Default for ColumnViewFrame {
        fn default() -> Self {
            Self {
                column_view: Default::default(),
                name_column: Default::default(),
                pid_column: Default::default(),
                cpu_column: Default::default(),
                memory_column: Default::default(),
                shared_memory_column: Default::default(),
                drive_column: Default::default(),
                network_usage_column: Default::default(),
                gpu_usage_column: Default::default(),
                gpu_memory_column: Default::default(),

                selected_item: RefCell::new(RowModelBuilder::new().build()),
                row_sorter: OnceCell::new(),

                use_merged_stats: Cell::new(false),

                show_column_separators: Cell::new(false),

                action_show_context_menu: Cell::new(gio::SimpleAction::new(
                    "show-context-menu",
                    Some(VariantTy::TUPLE),
                )),

                action_group: Cell::new(Default::default()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ColumnViewFrame {
        const NAME: &'static str = "ColumnViewFrame";
        type Type = super::ColumnViewFrame;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ColumnViewFrame {
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

            self.update_column_order();

            self.name_column
                .set_factory(Some(&name_list_item_factory()));
            self.name_column
                .set_sorter(Some(&name_sorter(&self.column_view)));

            self.pid_column.set_factory(Some(&pid_list_item_factory()));
            self.pid_column
                .set_sorter(Some(&pid_sorter(&self.column_view)));

            self.cpu_column.set_factory(Some(&cpu_list_item_factory()));
            self.cpu_column
                .set_sorter(Some(&cpu_sorter(&self.column_view)));

            self.memory_column
                .set_factory(Some(&memory_list_item_factory()));
            self.memory_column
                .set_sorter(Some(&memory_sorter(&self.column_view)));

            self.shared_memory_column
                .set_factory(Some(&shared_memory_list_item_factory()));
            self.shared_memory_column
                .set_sorter(Some(&shared_memory_sorter(&self.column_view)));

            self.drive_column
                .set_factory(Some(&drive_list_item_factory()));
            self.drive_column
                .set_sorter(Some(&drive_sorter(&self.column_view)));

            self.network_usage_column
                .set_factory(Some(&network_list_item_factory()));
            self.network_usage_column
                .set_sorter(Some(&network_sorter(&self.column_view)));

            self.gpu_usage_column
                .set_factory(Some(&gpu_list_item_factory()));
            self.gpu_usage_column
                .set_sorter(Some(&gpu_sorter(&self.column_view)));

            self.gpu_memory_column
                .set_factory(Some(&gpu_memory_list_item_factory()));
            self.gpu_memory_column
                .set_sorter(Some(&gpu_memory_sorter(&self.column_view)));

            let column_view_title = self.column_view.first_child();
            adjust_view_header_alignment(column_view_title);

            self.action_group()
                .add_action(self.action_show_context_menu());
            self.obj()
                .insert_action_group("apps-page", Some(self.action_group()));
        }
    }

    impl WidgetImpl for ColumnViewFrame {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl BoxImpl for ColumnViewFrame {}

    impl ColumnViewFrame {
        pub fn action_show_context_menu(&self) -> &gio::SimpleAction {
            unsafe { &*self.action_show_context_menu.as_ptr() }
        }

        pub fn action_group(&self) -> &gio::SimpleActionGroup {
            unsafe { &*self.action_group.as_ptr() }
        }

        pub fn setup(
            &self,
            section_item_1: &RowModel,
            section_item_2: &RowModel,
            process_action_bar: Option<&ProcessActionBar>,
            service_action_bar: Option<&ServiceActionBar>,
            service_toggle_group: Option<&ToggleGroup>,
        ) {
            let model = gio::ListStore::new::<RowModel>();
            model.append(section_item_1);
            model.append(section_item_2);

            let tree_model = Self::create_tree_model(model);
            let filter_list_model = self.configure_filter(tree_model, service_toggle_group);
            let (sort_list_model, row_sorter) = self.setup_filter_model(filter_list_model);
            let selection_model =
                self.setup_selection_model(sort_list_model, process_action_bar, service_action_bar);
            self.column_view.set_model(Some(&selection_model));

            let _ = self.row_sorter.set(row_sorter);

            selection_model.set_selected(0);

            if let Some(process_action_bar) = process_action_bar {
                process_action_bar.imp().configure(self);
                process_action_bar
                    .imp()
                    .handle_changed_selection(&self.selected_item.borrow());
            }

            if let Some(service_action_bar) = service_action_bar {
                service_action_bar.imp().configure(self);
                service_action_bar
                    .imp()
                    .handle_changed_selection(&self.selected_item.borrow());
            }

            configure_column_frame(self);
        }

        fn create_tree_model(model: impl IsA<gio::ListModel>) -> gtk::TreeListModel {
            gtk::TreeListModel::new(model, false, true, move |model_entry| {
                let Some(row_model) = model_entry.downcast_ref::<RowModel>() else {
                    return None;
                };
                Some(row_model.children().clone().into())
            })
        }

        fn configure_filter(
            &self,
            tree_list_model: impl IsA<gio::ListModel>,
            group: Option<&ToggleGroup>,
        ) -> gtk::FilterListModel {
            let Some(window) = app!().window() else {
                g_critical!(
                    "MissionCenter::ServicesPage",
                    "Failed to get MissionCenterWindow instance; searching and filtering will not function"
                );
                return gtk::FilterListModel::new(Some(tree_list_model), None::<gtk::CustomFilter>);
            };

            let filter = gtk::CustomFilter::new({
                let window = window.downgrade();
                let group = group.clone().map(|it| it.downgrade());
                move |obj| {
                    let Some(row_model) = obj
                        .downcast_ref::<gtk::TreeListRow>()
                        .and_then(|row| row.item())
                        .and_then(|item| item.downcast::<RowModel>().ok())
                    else {
                        return false;
                    };

                    let search = (|| {
                        let Some(window) = window.upgrade() else {
                            return true;
                        };

                        let window = window.imp();

                        if !window.search_button.is_active() {
                            return true;
                        }

                        if window.header_search_entry.text().is_empty() {
                            return true;
                        }

                        if row_model.content_type() == ContentType::SectionHeader {
                            return true;
                        }

                        let entry_name = row_model.name().to_lowercase();
                        let pid = row_model.pid().to_string();
                        let search_query = window.header_search_entry.text().to_lowercase();

                        if entry_name.contains(&search_query) || pid.contains(&search_query) {
                            return true;
                        }

                        if search_query.contains(&entry_name) || search_query.contains(&pid) {
                            return true;
                        }

                        let str_distance = Levenshtein::default()
                            .for_str(&entry_name, &search_query)
                            .ndist();
                        if str_distance <= 0.6 {
                            return true;
                        }

                        false
                    })();

                    let filter = (|group: Option<&WeakRef<ToggleGroup>>| {
                        let Some(group) = group.map(|it| it.upgrade()).flatten() else {
                            return true;
                        };

                        if row_model.content_type() == ContentType::Service {
                            return match group.active() {
                                0 => true,
                                1 => row_model.service_running(),
                                2 => row_model.service_failed(),
                                3 => row_model.service_stopped(),
                                4 => !row_model.service_enabled() && !row_model.service_enabled(),
                                _ => true,
                            };
                        }
                        true
                    })(group.as_ref());

                    search && filter
                }
            });

            window.imp().header_search_entry.connect_search_changed({
                let filter = filter.downgrade();
                move |_| {
                    if let Some(filter) = filter.upgrade() {
                        filter.changed(gtk::FilterChange::Different);
                    }
                }
            });

            group.map(|it| {
                it.connect_active_notify({
                    let filter = filter.downgrade();
                    move |_| {
                        if let Some(filter) = filter.upgrade() {
                            filter.changed(gtk::FilterChange::Different);
                        }
                    }
                })
            });

            gtk::FilterListModel::new(Some(tree_list_model), Some(filter))
        }

        fn setup_filter_model(
            &self,
            filter_list_model: impl IsA<gio::ListModel>,
        ) -> (gtk::SortListModel, gtk::TreeListRowSorter) {
            let column_view_sorter = self.column_view.sorter();

            if let Some(column_view_sorter) = column_view_sorter.as_ref() {
                column_view_sorter.connect_changed({
                    |sorter, _| {
                        let settings = settings!();

                        let Some(sorter) = sorter.downcast_ref::<gtk::ColumnViewSorter>() else {
                            return;
                        };

                        let Some(sorted_column) = sorter.primary_sort_column() else {
                            return;
                        };

                        let Some(sorted_column_id) = sorted_column.id() else {
                            return;
                        };
                        let _ = settings
                            .set_string("apps-page-sorting-column-name", sorted_column_id.as_str());

                        let sort_order = sorter.primary_sort_order();
                        let _ = settings.set_enum(
                            "apps-page-sorting-order",
                            match sort_order {
                                gtk::SortType::Ascending => gtk::ffi::GTK_SORT_ASCENDING,
                                gtk::SortType::Descending => gtk::ffi::GTK_SORT_DESCENDING,
                                _ => gtk::ffi::GTK_SORT_ASCENDING,
                            },
                        );
                    }
                });
            }

            let tree_list_sorter = gtk::TreeListRowSorter::new(column_view_sorter);
            (
                gtk::SortListModel::new(Some(filter_list_model), Some(tree_list_sorter.clone())),
                tree_list_sorter,
            )
        }

        fn setup_selection_model(
            &self,
            sort_list_model: impl IsA<gio::ListModel>,
            process_action_bar: Option<&ProcessActionBar>,
            service_action_bar: Option<&ServiceActionBar>,
        ) -> gtk::SingleSelection {
            let selection_model = gtk::SingleSelection::new(Some(sort_list_model));
            selection_model.set_autoselect(true);

            let this = self.obj().downgrade();

            // Create weak references upfront to avoid temporary value issues
            let process_action_bar_weak = process_action_bar.map(|it| it.downgrade());
            let service_action_bar_weak = service_action_bar.map(|it| it.downgrade());

            selection_model.connect_selected_item_notify({
                move |model| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };

                    let imp = this.imp();

                    let Some(row_model) = model
                        .selected_item()
                        .and_then(|item| item.downcast::<gtk::TreeListRow>().ok())
                        .and_then(|row| row.item())
                        .and_then(|obj| obj.downcast::<RowModel>().ok())
                    else {
                        return;
                    };

                    if let Some(process_action_bar) =
                        process_action_bar_weak.as_ref().and_then(|it| it.upgrade())
                    {
                        process_action_bar
                            .imp()
                            .handle_changed_selection(&row_model);
                    }

                    if let Some(service_action_bar) =
                        service_action_bar_weak.as_ref().and_then(|it| it.upgrade())
                    {
                        service_action_bar
                            .imp()
                            .handle_changed_selection(&row_model);
                    }

                    imp.selected_item.replace(row_model);
                }
            });

            selection_model
        }

        pub fn update_column_titles(&self, readings: &crate::magpie_client::Readings) {
            let mut buffer = ArrayString::<128>::new();

            let cpu_usage = readings.cpu.total_usage_percent.round() as u32;
            let _ = write!(&mut buffer, "{}\n{}%", i18n("CPU"), cpu_usage);
            self.cpu_column.set_title(Some(buffer.as_str()));

            buffer.clear();

            let mem_total = if readings.mem_info.mem_total > 0 {
                readings.mem_info.mem_total
            } else {
                1
            };

            // https://gitlab.com/procps-ng/procps/-/blob/master/library/meminfo.c?ref_type=heads#L736
            let mem_avail = if readings.mem_info.mem_available > readings.mem_info.mem_total {
                readings.mem_info.mem_free
            } else {
                readings.mem_info.mem_available
            };

            let memory_used = mem_total.saturating_sub(mem_avail);
            let memory_usage = memory_used as f32 * 100. / mem_total as f32;
            let memory_usage = memory_usage.round() as u32;
            let _ = write!(&mut buffer, "{}\n{}%", i18n("Memory"), memory_usage);
            self.memory_column.set_title(Some(buffer.as_str()));

            buffer.clear();
            if readings.disks_info.is_empty() {
                let _ = write!(&mut buffer, "{}\n0%", i18n("Drive"));
            } else {
                let mut sum = 0.;
                for disk in &readings.disks_info {
                    sum += disk.busy_percent
                }
                let drive_usage = sum / readings.disks_info.len() as f32;
                let drive_usage = drive_usage.round() as u32;
                let _ = write!(&mut buffer, "{}\n{}%", i18n("Drive"), drive_usage);
            }
            self.drive_column.set_title(Some(buffer.as_str()));

            buffer.clear();
            if readings.running_processes.is_empty() {
                let _ = write!(&mut buffer, "{}\n0", i18n("Network"));
            } else {
                let mut sum = 0.;
                for proc in readings.running_processes.values() {
                    sum += proc.usage_stats.network_usage.round();
                }

                let label = crate::to_human_readable_nice(
                    sum,
                    &DataType::NetworkBytesPerSecond,
                    &settings!(),
                );

                let _ = write!(&mut buffer, "{}\n{}", i18n("Network"), label);
            }
            self.network_usage_column.set_title(Some(buffer.as_str()));

            buffer.clear();
            if readings.gpus.is_empty() {
                let _ = write!(&mut buffer, "{}\n0%", i18n("GPU"));
                self.gpu_usage_column.set_title(Some(buffer.as_str()));

                buffer.clear();
                let _ = write!(&mut buffer, "{}\n0%", i18n("GPU Memory"));
                self.gpu_memory_column.set_title(Some(buffer.as_str()));
            } else {
                let mut sum_util = 0.;
                let mut sum_mem_used = 0.;
                let mut sum_mem_total = 0.;
                for gpu in readings.gpus.values() {
                    sum_util += gpu.utilization_percent.unwrap_or(0.);
                    sum_mem_used += gpu.used_memory.unwrap_or(0) as f32;
                    sum_mem_total += gpu.total_memory.unwrap_or(0) as f32;
                }
                let gpu_usage = sum_util / readings.gpus.len() as f32;
                let gpu_usage = gpu_usage.round() as u32;
                let _ = write!(&mut buffer, "{}\n{}%", i18n("GPU"), gpu_usage);
                self.gpu_usage_column.set_title(Some(buffer.as_str()));

                buffer.clear();
                let gpu_mem_usage = sum_mem_used * 100. / sum_mem_total;
                let gpu_mem_usage = gpu_mem_usage.round() as u32;
                let _ = write!(&mut buffer, "{}\n{}%", i18n("GPU Memory"), gpu_mem_usage);
                self.gpu_memory_column.set_title(Some(buffer.as_str()));
            }
        }

        pub fn update_column_order(&self) {
            let column_view = &self.column_view;

            let settings = settings!();

            if settings.boolean("apps-page-remember-column-order") {
                let columns = column_view.columns();
                let mut all_columns = Vec::new();
                for i in 0..columns.n_items() {
                    let Some(column) = columns
                        .item(i)
                        .and_then(|c| c.downcast::<gtk::ColumnViewColumn>().ok())
                    else {
                        continue;
                    };
                    all_columns.push(column);
                }
                for column in &all_columns {
                    column_view.remove_column(column);
                }

                let setting_column_order = settings.string("apps-page-column-order");
                for column_id in setting_column_order.split(';') {
                    let Some((index, column)) = all_columns
                        .iter()
                        .enumerate()
                        .find(|(_, c)| {
                            let Some(id) = c.id() else {
                                return false;
                            };
                            id == column_id
                        })
                        .map(|(index, c)| (index, c.clone()))
                    else {
                        continue;
                    };
                    all_columns.remove(index);
                    column_view.append_column(&column);
                }

                for column in all_columns.drain(..) {
                    column_view.append_column(&column);
                }
            } else {
                let _ = settings.set_string("apps-page-column-order", "");
            }

            column_view
                .columns()
                .connect_items_changed(|model, _, _, _| {
                    let settings = settings!();

                    let mut order = String::new();
                    for i in 0..model.n_items() {
                        let Some(id) = model
                            .item(i)
                            .and_then(|c| c.downcast::<gtk::ColumnViewColumn>().ok())
                            .and_then(|c| c.id())
                        else {
                            continue;
                        };

                        order.push_str(id.as_str());
                        order.push(';');
                    }
                    order.pop();

                    let _ = settings.set_string("apps-page-column-order", order.as_str());
                });
        }
    }
}

glib::wrapper! {
    pub struct ColumnViewFrame(ObjectSubclass<imp::ColumnViewFrame>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}
