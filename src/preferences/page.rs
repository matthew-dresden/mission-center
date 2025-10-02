/* preferences/page.rs
 *
 * Copyright 2023 Romeo Calota
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

use adw::{prelude::*, subclass::prelude::*, SpinRow, SwitchRow};
use gtk::{gio, glib, Scale};

use crate::settings;

const MAX_INTERVAL_TICKS: u64 = 200;
const MIN_INTERVAL_TICKS: u64 = 10;

const MAX_POINTS: i32 = 600;
const MIN_POINTS: i32 = 10;

#[derive(Debug)]
enum PointsUpdateSource {
    DataPoints,
    SettingsLoad,
    UpdateInterval,
    MinutesChanged,
    SecondsChanged,
}

macro_rules! connect_switch_to_setting {
    ($this: expr, $switch_row: expr, $setting: literal) => {
        $switch_row.connect_active_notify({
            move |switch_row| {
                if let Err(e) = settings!().set_boolean($setting, switch_row.is_active()) {
                    gtk::glib::g_critical!(
                        "MissionCenter::Preferences",
                        "Failed to set {} setting: {}",
                        $setting,
                        e
                    );
                }
            }
        });
    };
}

macro_rules! connect_toggle_pair_to_setting {
    ($this: expr, $toggle_group: expr, $toggle_truthy: expr, $setting: literal) => {
        $toggle_group.connect_notify_local(Some("active"), {
            let toggle_truthy = $toggle_truthy.downgrade();
            move |toggle_group, _| {
                let Some(toggle_truthy) = toggle_truthy.upgrade() else {
                    return;
                };

                let active_index = toggle_group.active();
                let active_toggle = toggle_group.toggle(active_index);
                let truthy_active = active_toggle.as_ref() == Some(&toggle_truthy);
                if let Err(e) = settings!().set_boolean($setting, truthy_active) {
                    gtk::glib::g_critical!(
                        "MissionCenter::Preferences",
                        "Failed to set {} setting: {}",
                        $setting,
                        e
                    );
                }
            }
        });
    };
}

mod imp {
    use std::sync::{LazyLock, Mutex, TryLockResult};
    use super::*;
    use gtk::SpinButton;
    use crate::application::INTERVAL_STEP;
    use crate::preferences::page::PointsUpdateSource::SettingsLoad;

    #[derive(gtk::CompositeTemplate, Default)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/preferences/page.ui")]
    pub struct PreferencesPage {
        #[template_child]
        pub update_interval: TemplateChild<SpinRow>,
        #[template_child]
        pub data_points: TemplateChild<Scale>,
        #[template_child]
        pub range_minutes: TemplateChild<SpinButton>,
        #[template_child]
        pub range_seconds: TemplateChild<SpinButton>,

        #[template_child]
        pub smooth_graphs: TemplateChild<SwitchRow>,
        #[template_child]
        pub sliding_graphs: TemplateChild<SwitchRow>,
        #[template_child]
        pub network_dynamic_scaling: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_cpu: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_memory: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_disks: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_network: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_gpus: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_fans: TemplateChild<SwitchRow>,

        #[template_child]
        pub merged_process_stats: TemplateChild<SwitchRow>,
        #[template_child]
        pub remember_sorting: TemplateChild<SwitchRow>,
        #[template_child]
        pub remember_column_order: TemplateChild<SwitchRow>,
        #[template_child]
        pub core_count_affects_percentages: TemplateChild<SwitchRow>,
        #[template_child]
        pub show_column_separators: TemplateChild<SwitchRow>,

        #[template_child]
        pub toggle_group_memory_unit: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_memory_unit_bits: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_memory_unit_bytes: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_group_memory_base: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_memory_base_2: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_memory_base_10: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_group_drive_unit: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_drive_unit_bits: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_drive_unit_bytes: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_group_drive_base: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_drive_base_2: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_drive_base_10: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_group_net_unit: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_net_unit_bits: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_net_unit_bytes: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_group_net_base: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub toggle_net_base_2: TemplateChild<adw::Toggle>,
        #[template_child]
        pub toggle_net_base_10: TemplateChild<adw::Toggle>,

        pub interval_updating: Mutex<bool>,
    }

    impl PreferencesPage {
        fn configure_minutes_seconds_default(&self) {
            let interval = self.update_interval.value();

            self.configure_minutes_seconds(interval);
        }

        fn configure_minutes_seconds(&self, interval: f64) {
            let max_seconds = MAX_POINTS as f64 * interval;

            let set_minutes = self.range_minutes.value();

            self.range_minutes.set_range(0., (max_seconds / 60.).ceil());

            let min_secs = (set_minutes * 60. / interval).ceil() * interval % 60.;
            let max_secs = 60. - ((set_minutes + 1.) * 60. / interval).floor() * interval % 60.;

            let min_secs = min_secs.max((MIN_POINTS as f64) * interval - set_minutes * 60.);
            let max_secs = max_secs.min(max_seconds - set_minutes * 60.);

            assert!(max_secs <= 60.);
            assert!(min_secs >= 0.);

            self.range_seconds.adjustment().set_step_increment(interval);
            self.range_seconds.set_range(min_secs, max_secs);

            if self.range_seconds.value() < min_secs {
                self.range_seconds.set_value(min_secs);
            } else if self.range_seconds.value() > max_secs {
                self.range_seconds.set_value(max_secs);
            }
        }

        pub fn configure_update_speed(&self, source: PointsUpdateSource) {
            let mut guard = match self.interval_updating.try_lock() {
                Ok(g) => {g}
                Err(_) => {
                    return;
                }
            };

            use crate::application::INTERVAL_STEP;
            use glib::g_critical;

            let settings = settings!();

            println!("Configurating {:?}\t{}\t{}\t{}", source, self.range_minutes.value(), self.range_seconds.value(), self.data_points.value());

            match source {
                PointsUpdateSource::SettingsLoad => {
                    let new_points = settings.int("performance-page-data-points");

                    let interval = settings.uint64("app-update-interval-u64") as f64 * INTERVAL_STEP;

                    let seconds = (new_points as f64) * interval;

                    self.range_minutes.set_value((seconds / 60.).floor());
                    self.range_seconds.set_value(seconds % 60.);

                    self.configure_minutes_seconds(interval);
                }
                PointsUpdateSource::UpdateInterval => {
                    let new_interval = (self.update_interval.value() / INTERVAL_STEP).round() as u64;

                    if settings
                        .set_uint64("app-update-interval-u64", new_interval)
                        .is_err()
                    {
                        g_critical!(
                            "MissionCenter::Preferences",
                            "Failed to set update interval setting",
                        );
                    }

                    let points = (self.data_points.value() as i32).clamp(MIN_POINTS, MAX_POINTS);

                    let seconds = (points as u64 * new_interval) as f64 * INTERVAL_STEP;
                    let max_seconds = (MAX_POINTS as u64 * new_interval) as f64 * INTERVAL_STEP;

                    self.range_seconds.adjustment().set_step_increment(new_interval as f64 * INTERVAL_STEP);

                    self.range_minutes.set_value((seconds / 60.).floor());
                    self.range_seconds.set_value(seconds % 60.);

                    self.configure_minutes_seconds_default();
                }
                PointsUpdateSource::DataPoints => {
                    let new_points = (self.data_points.value() as i32).clamp(MIN_POINTS, MAX_POINTS);

                    if settings
                        .set_int("performance-page-data-points", new_points)
                        .is_err()
                    {
                        g_critical!(
                            "MissionCenter::Preferences",
                            "Failed to set update points setting",
                        );
                    }

                    let interval = self.update_interval.value();

                    let seconds = (new_points as f64) * interval;

                    self.range_minutes.set_value((seconds / 60.).floor());
                    self.range_seconds.set_value(seconds % 60.);

                    self.configure_minutes_seconds_default();
                }
                PointsUpdateSource::MinutesChanged | PointsUpdateSource::SecondsChanged => {
                    let interval = self.update_interval.value() ;

                    let seconds = self.range_minutes.value() * 60. + self.range_seconds.value();

                    let new_data_points =
                        ((seconds / interval).round() as i32).clamp(MIN_POINTS, MAX_POINTS);

                    println!("New points {new_data_points}");

                    self.data_points.set_value(new_data_points as f64);

                    if settings
                        .set_int("performance-page-data-points", new_data_points)
                        .is_err()
                    {
                        g_critical!(
                            "MissionCenter::Preferences",
                            "Failed to set update points setting",
                        );
                    }

                    self.configure_minutes_seconds_default();
                }
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesPage {
        const NAME: &'static str = "PreferencesPage";
        type Type = super::PreferencesPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            SwitchRow::ensure_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesPage {
        fn constructed(&self) {
            self.parent_constructed();

            self.data_points
                .downcast_ref::<Scale>()
                .unwrap()
                .connect_value_changed({
                    let this = self.obj().downgrade();
                    move |_| {
                        if let Some(this) = this.upgrade() {
                            this.imp().configure_update_speed(PointsUpdateSource::DataPoints);
                        }
                    }
                });

            self.update_interval
                .downcast_ref::<SpinRow>()
                .unwrap()
                .connect_changed({
                    let this = self.obj().downgrade();
                    move |_| {
                        if let Some(this) = this.upgrade() {
                            this.imp()
                                .configure_update_speed(PointsUpdateSource::UpdateInterval);
                        }
                    }
                });

            self.range_minutes
                .downcast_ref::<SpinButton>()
                .unwrap()
                .connect_changed({
                    let this = self.obj().downgrade();
                    move |_| {
                        if let Some(this) = this.upgrade() {
                            this.imp()
                                .configure_update_speed(PointsUpdateSource::MinutesChanged);
                        }
                    }
                });

            self.range_seconds
                .downcast_ref::<SpinButton>()
                .unwrap()
                .connect_changed({
                    let this = self.obj().downgrade();
                    move |_| {
                        if let Some(this) = this.upgrade() {
                            this.imp()
                                .configure_update_speed(PointsUpdateSource::SecondsChanged);
                        }
                    }
                });

            connect_switch_to_setting!(self, self.smooth_graphs, "performance-smooth-graphs");
            connect_switch_to_setting!(self, self.sliding_graphs, "performance-sliding-graphs");
            connect_switch_to_setting!(
                self,
                self.network_dynamic_scaling,
                "performance-page-network-dynamic-scaling"
            );
            connect_switch_to_setting!(self, self.show_cpu, "performance-show-cpu");
            connect_switch_to_setting!(self, self.show_memory, "performance-show-memory");
            connect_switch_to_setting!(self, self.show_disks, "performance-show-disks");
            connect_switch_to_setting!(self, self.show_network, "performance-show-network");
            connect_switch_to_setting!(self, self.show_gpus, "performance-show-gpus");
            connect_switch_to_setting!(self, self.show_fans, "performance-show-fans");

            connect_switch_to_setting!(
                self,
                self.merged_process_stats,
                "apps-page-merged-process-stats"
            );
            connect_switch_to_setting!(self, self.remember_sorting, "apps-page-remember-sorting");
            connect_switch_to_setting!(
                self,
                self.remember_column_order,
                "apps-page-remember-column-order"
            );
            connect_switch_to_setting!(
                self,
                self.core_count_affects_percentages,
                "apps-page-core-count-affects-percentages"
            );
            connect_switch_to_setting!(
                self,
                self.show_column_separators,
                "apps-page-show-column-separators"
            );

            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_memory_unit,
                self.toggle_memory_unit_bytes,
                "performance-page-memory2-use-bytes"
            );
            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_memory_base,
                self.toggle_memory_base_2,
                "performance-page-memory2-use-base2"
            );
            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_drive_unit,
                self.toggle_drive_unit_bytes,
                "performance-page-drive-use-bytes"
            );
            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_drive_base,
                self.toggle_drive_base_2,
                "performance-page-drive-use-base2"
            );
            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_net_unit,
                self.toggle_net_unit_bytes,
                "performance-page-network-use-bytes"
            );
            connect_toggle_pair_to_setting!(
                self,
                self.toggle_group_net_base,
                self.toggle_net_base_2,
                "performance-page-network-use-base2"
            );

            self.configure_update_speed(SettingsLoad);
        }
    }

    impl WidgetImpl for PreferencesPage {}

    impl PreferencesPageImpl for PreferencesPage {}
}

glib::wrapper! {
    pub struct PreferencesPage(ObjectSubclass<imp::PreferencesPage>)
        @extends adw::PreferencesPage, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl PreferencesPage {
    pub fn new() -> Self {
        let this: Self = glib::Object::builder().build();

        this.set_initial_update_speed();

        let imp = this.imp();
        let settings = settings!();

        imp.smooth_graphs
            .set_active(settings.boolean("performance-smooth-graphs"));
        imp.sliding_graphs
            .set_active(settings.boolean("performance-sliding-graphs"));
        imp.network_dynamic_scaling
            .set_active(settings.boolean("performance-page-network-dynamic-scaling"));
        imp.show_cpu
            .set_active(settings.boolean("performance-show-cpu"));
        imp.show_memory
            .set_active(settings.boolean("performance-show-memory"));
        imp.show_disks
            .set_active(settings.boolean("performance-show-disks"));
        imp.show_network
            .set_active(settings.boolean("performance-show-network"));
        imp.show_gpus
            .set_active(settings.boolean("performance-show-gpus"));
        imp.show_fans
            .set_active(settings.boolean("performance-show-fans"));

        imp.merged_process_stats
            .set_active(settings.boolean("apps-page-merged-process-stats"));
        imp.remember_sorting
            .set_active(settings.boolean("apps-page-remember-sorting"));
        imp.remember_column_order
            .set_active(settings.boolean("apps-page-remember-column-order"));
        imp.core_count_affects_percentages
            .set_active(settings.boolean("apps-page-core-count-affects-percentages"));
        imp.show_column_separators
            .set_active(settings.boolean("apps-page-show-column-separators"));

        imp.toggle_group_memory_unit
            .set_active(!settings.boolean("performance-page-memory2-use-bytes") as u32);
        imp.toggle_group_memory_base
            .set_active(settings.boolean("performance-page-memory2-use-base2") as u32);
        imp.toggle_group_drive_unit
            .set_active(!settings.boolean("performance-page-drive-use-bytes") as u32);
        imp.toggle_group_drive_base
            .set_active(settings.boolean("performance-page-drive-use-base2") as u32);
        imp.toggle_group_net_unit
            .set_active(!settings.boolean("performance-page-network-use-bytes") as u32);
        imp.toggle_group_net_base
            .set_active(settings.boolean("performance-page-network-use-base2") as u32);

        this
    }

    fn set_initial_update_speed(&self) {
        use crate::application::INTERVAL_STEP;

        let settings = settings!();

        let data_points = settings.int("performance-page-data-points");
        let update_interval_s = (settings.uint64("app-update-interval-u64") as f64) * INTERVAL_STEP;
        let this = self.imp();

        this.data_points.set_value(data_points as f64);
        this.update_interval.set_value(update_interval_s);
    }
}
