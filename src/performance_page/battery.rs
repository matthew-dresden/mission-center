/* performance_page/battery.rs
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

use adw;
use adw::subclass::prelude::*;
use glib::{ParamSpec, Properties, Value};
use gtk::{gio, glib, prelude::*};

use magpie_types::battery::Battery;

use super::widgets::GraphWidget;
use crate::application::INTERVAL_STEP;
use crate::i18n::*;
use crate::performance_page::{PageExt, MK_TO_0_C};
use crate::to_short_human_readable_time;
use crate::performance_page::widgets::DatasetGroup;
use crate::performance_page::widgets::ScalingSettings;

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::PerformancePageBattery)]
    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/performance_page/battery.ui")]
    pub struct PerformancePageBattery {
        #[template_child]
        pub title_battery_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub title_battery_model: TemplateChild<gtk::Label>,
        #[template_child]
        pub energy_rate_graph: TemplateChild<GraphWidget>,

        #[template_child]
        pub energy_rate_max_y: TemplateChild<gtk::Label>,
        #[template_child]
        pub energy_rate_max_duration: TemplateChild<gtk::Label>,
        #[template_child]
        pub energy_rate_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub context_menu: TemplateChild<gtk::Popover>,

        #[property(get = Self::name, set = Self::set_name, type = String)]
        name: RefCell<String>,
        #[property(get, set)]
        base_color: Cell<gtk::gdk::RGBA>,
        #[property(get, set)]
        summary_mode: Cell<bool>,

        #[property(get = Self::infobar_content, type = Option < gtk::Widget >)]
        pub infobar_content: OnceCell<gtk::Grid>,

        pub percentage: OnceCell<gtk::Label>,
        pub energy: OnceCell<gtk::Label>,
        pub power: OnceCell<gtk::Label>,
        pub voltage: OnceCell<gtk::Label>,
        pub time_to: OnceCell<gtk::Label>,
        pub time_to_direction: OnceCell<gtk::Label>,
        pub state: OnceCell<gtk::Label>,
        pub charge_cycles: OnceCell<gtk::Label>,

        pub technology: OnceCell<gtk::Label>,
        pub capacity: OnceCell<gtk::Label>,
        pub energy_empty: OnceCell<gtk::Label>,
        pub energy_full: OnceCell<gtk::Label>,
        pub energy_full_design: OnceCell<gtk::Label>,
        pub voltage_min_design: OnceCell<gtk::Label>,
    }

    impl Default for PerformancePageBattery {
        fn default() -> Self {
            Self {
                title_battery_name: Default::default(),
                title_battery_model: Default::default(),
                energy_rate_graph: Default::default(),
                energy_rate_max_y: Default::default(),
                energy_rate_max_duration: Default::default(),
                energy_rate_box: Default::default(),
                context_menu: Default::default(),

                name: RefCell::new(String::new()),
                base_color: Cell::new(gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)),
                summary_mode: Cell::new(false),

                infobar_content: Default::default(),

                percentage: Default::default(),
                energy: Default::default(),
                power: Default::default(),
                voltage: Default::default(),
                time_to: Default::default(),
                time_to_direction: Default::default(),
                state: Default::default(),
                charge_cycles: Default::default(),

                technology: Default::default(),
                capacity: Default::default(),
                energy_empty: Default::default(),
                energy_full: Default::default(),
                energy_full_design: Default::default(),
                voltage_min_design: Default::default(),
            }
        }
    }

    impl PerformancePageBattery {
        fn name(&self) -> String {
            self.name.borrow().clone()
        }

        fn set_name(&self, name: String) {
            if name == *self.name.borrow() {
                return;
            }

            self.name.replace(name);
        }

        fn infobar_content(&self) -> Option<gtk::Widget> {
            self.infobar_content.get().map(|ic| ic.clone().into())
        }
    }

    impl PerformancePageBattery {
        fn configure_actions(this: &super::PerformancePageBattery) {
            let actions = gio::SimpleActionGroup::new();
            this.insert_action_group("graph", Some(&actions));

            let action = gio::SimpleAction::new("copy", None);
            action.connect_activate({
                let this = this.downgrade();
                move |_, _| {
                    if let Some(this) = this.upgrade() {
                        let clipboard = this.clipboard();
                        clipboard.set_text(this.imp().data_summary().as_str());
                    }
                }
            });
            actions.add_action(&action);
        }

        fn configure_context_menu(this: &super::PerformancePageBattery) {
            let right_click_controller = gtk::GestureClick::new();
            right_click_controller.set_button(3); // Secondary click (AKA right click)
            right_click_controller.connect_released({
                let this = this.downgrade();
                move |_click, _n_press, x, y| {
                    if let Some(this) = this.upgrade() {
                        let this = this.imp();
                        this.context_menu
                            .set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                                x.round() as i32,
                                y.round() as i32,
                                1,
                                1,
                            )));
                        this.context_menu.popup();
                    }
                }
            });
            this.add_controller(right_click_controller);
        }
    }

    impl PerformancePageBattery {
        pub fn set_static_information(this: &super::PerformancePageBattery, battery: &Battery) -> bool {
            let t = this.clone();

            let this = this.imp();

            this.energy_rate_graph.connect_local("resize", true, move |_| {
                let this = t.imp();

                {
                    let width = this.energy_rate_graph.width() as f32;
                    let height = this.energy_rate_graph.height() as f32;

                    let mut a = width;
                    let mut b = height;
                    if width > height {
                        a = height;
                        b = width;
                    }

                    this.energy_rate_graph
                        .set_vertical_line_count((width * (a / b) / 30.).round().max(5.) as u32);
                }

                None
            });

            this.title_battery_model.set_text(&battery.model);

            if let Some(technology) = this.technology.get() {
                if let Some(tech) = &battery.technology {
                    technology.set_text(tech)
                } else {
                    technology.set_visible(false)
                }
            }

            if let Some(capacity) = this.capacity.get() {
                if let Some(v) = &battery.capacity {
                    capacity.set_text(&format!("{:.0}%", v * 100.))
                } else {
                    capacity.set_visible(false)
                }
            }

            if let Some(energy_empty) = this.energy_empty.get() {
                if let Some(v) = &battery.energy_empty {
                    energy_empty.set_text(&format!("{} mWh", v))
                } else {
                    energy_empty.set_visible(false)
                }
            }

            if let Some(energy_full) = this.energy_full.get() {
                if let Some(v) = &battery.energy_full {
                    energy_full.set_text(&format!("{} mWh", v))
                } else {
                    energy_full.set_visible(false)
                }
            }

            if let Some(energy_full_design) = this.energy_full_design.get() {
                if let Some(v) = &battery.energy_full_design {
                    energy_full_design.set_text(&format!("{} mWh", v))
                } else {
                    energy_full_design.set_visible(false)
                }
            }

            if let Some(voltage_min_design) = this.voltage_min_design.get() {
                if let Some(v) = &battery.voltage_min_design {
                    voltage_min_design.set_text(&format!("{:.1} V", v))
                } else {
                    voltage_min_design.set_visible(false)
                }
            }

            true
        }

        pub fn update_readings(
            this: &super::PerformancePageBattery,
            battery: &Battery,
            index: Option<usize>,
        ) -> bool {
            let this = this.imp();

            if let Some(percentage) = this.percentage.get() {
                percentage.set_text(&i18n_f("{}%", &[&format!("{:.0}", battery.percentage * 100.)]));
            }

            if let Some(energy) = this.energy.get() {
                if let Some(v) = &battery.energy {
                    energy.set_text(&format!("{} mWh", v))
                } else {
                    energy.set_visible(false)
                }
            }

            if let Some(power) = this.power.get() {
                if let Some(v) = &battery.power {
                    power.set_text(&format!("{:.1} W", v))
                } else {
                    power.set_visible(false)
                }
            }

            if let Some(voltage) = this.voltage.get() {
                if let Some(v) = &battery.voltage {
                    voltage.set_text(&format!("{:.1} V", v))
                } else {
                    voltage.set_visible(false)
                }
            }

            if let Some(time_to) = this.time_to.get() {
                if let Some(time_to_direction) = this.time_to_direction.get() {
                    if let Some(v) = &battery.time_to_full {
                        time_to.set_text(&format!("{}s", v));
                        time_to_direction.set_text("full");
                    }
                    if let Some(v) = &battery.time_to_empty {
                        time_to.set_text(&format!("{}s", v));
                        time_to_direction.set_text("empty");
                    }
                    else {
                        time_to.set_visible(false)
                    }
                }
            }

            //if let Some(state) = this.state.get() {
                //if let Some(v) = &battery.state {
                    //state.set_text(&format!("{} mWh", v))
                //} else {
                    //state.set_visible(false)
                //}
            //}

            if let Some(charge_cycles) = this.charge_cycles.get() {
                if let Some(v) = &battery.charge_cycles {
                    charge_cycles.set_text(&v.to_string())
                } else {
                    charge_cycles.set_visible(false)
                }
            }

            true
        }

        pub fn update_animations(this: &super::PerformancePageBattery, new_ticks: f32) -> bool {
            let this = this.imp();

            this.energy_rate_graph.update_animation(new_ticks);

            true
        }

        fn data_summary(&self) -> String {
            let unsupported = i18n("Unsupported");
            let unsupported = glib::GString::from(unsupported);

            //format!(
                //r#"Fan

    //{}
    //{}

    //Speed:               {}
    //PWM Percentage:      {}
    //Current Temperature: {}"#,
                //self.title_fan_name.text(),
                //self.title_temp_name.text(),
                //self.speed
                    //.get()
                    //.map(|s| s.text())
                    //.unwrap_or(unsupported.clone()),
                //self.pwm
                    //.get()
                    //.and_then(|pwm| if !pwm.is_visible() { None } else { Some(pwm) })
                    //.map(|s| s.text())
                    //.unwrap_or(unsupported.clone()),
                //self.temp
                    //.get()
                    //.and_then(|temp| if !temp.is_visible() { None } else { Some(temp) })
                    //.map(|s| s.text())
                    //.unwrap_or(unsupported)
            //)
            String::new()
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PerformancePageBattery {
        const NAME: &'static str = "PerformancePageBattery";
        type Type = super::PerformancePageBattery;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PerformancePageBattery {
        fn properties() -> &'static [ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &Value, pspec: &ParamSpec) {
            self.derived_set_property(id, value, pspec);
        }

        fn property(&self, id: usize, pspec: &ParamSpec) -> Value {
            self.derived_property(id, pspec)
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            let this = obj.upcast_ref::<super::PerformancePageBattery>().clone();

            Self::configure_actions(&this);
            Self::configure_context_menu(&this);

            let sidebar_content_builder = gtk::Builder::from_resource(
                "/io/missioncenter/MissionCenter/ui/performance_page/battery_details.ui",
            );

            let _ = self.infobar_content.set(
                sidebar_content_builder
                    .object::<gtk::Grid>("root")
                    .expect("Could not find `root` object in details pane"),
            );

            let _ = self.percentage.set(
                sidebar_content_builder
                    .object::<gtk::Label>("percentage")
                    .expect("Could not find `percentage` object in details pane"),
            );
            let _ = self.energy.set(
                sidebar_content_builder
                    .object::<gtk::Label>("energy")
                    .expect("Could not find `energy` object in details pane"),
            );
            let _ = self.power.set(
                sidebar_content_builder
                    .object::<gtk::Label>("power")
                    .expect("Could not find `power` object in details pane"),
            );
            let _ = self.voltage.set(
                sidebar_content_builder
                    .object::<gtk::Label>("voltage")
                    .expect("Could not find `voltage` object in details pane"),
            );
            let _ = self.time_to.set(
                sidebar_content_builder
                    .object::<gtk::Label>("time_to")
                    .expect("Could not find `time_to` object in details pane"),
            );
            let _ = self.time_to_direction.set(
                sidebar_content_builder
                    .object::<gtk::Label>("time_to_direction")
                    .expect("Could not find `time_to_direction` object in details pane"),
            );
            let _ = self.state.set(
                sidebar_content_builder
                    .object::<gtk::Label>("state")
                    .expect("Could not find `state` object in details pane"),
            );
            let _ = self.charge_cycles.set(
                sidebar_content_builder
                    .object::<gtk::Label>("charge_cycles")
                    .expect("Could not find `charge_cycles` object in details pane"),
            );
            let _ = self.technology.set(
                sidebar_content_builder
                    .object::<gtk::Label>("technology")
                    .expect("Could not find `technology` object in details pane"),
            );
            let _ = self.capacity.set(
                sidebar_content_builder
                    .object::<gtk::Label>("capacity")
                    .expect("Could not find `capacity` object in details pane"),
            );
            let _ = self.energy_empty.set(
                sidebar_content_builder
                    .object::<gtk::Label>("energy_empty")
                    .expect("Could not find `energy_empty` object in details pane"),
            );
            let _ = self.energy_full.set(
                sidebar_content_builder
                    .object::<gtk::Label>("energy_full")
                    .expect("Could not find `energy_full` object in details pane"),
            );
            let _ = self.energy_full_design.set(
                sidebar_content_builder
                    .object::<gtk::Label>("energy_full_design")
                    .expect("Could not find `energy_full_design` object in details pane"),
            );
            let _ = self.voltage_min_design.set(
                sidebar_content_builder
                    .object::<gtk::Label>("voltage_min_design")
                    .expect("Could not find `voltage_min_design` object in details pane"),
            );
        }
    }

    impl WidgetImpl for PerformancePageBattery {}

    impl BoxImpl for PerformancePageBattery {}
}

glib::wrapper! {
    pub struct PerformancePageBattery(ObjectSubclass<imp::PerformancePageBattery>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl PageExt for PerformancePageBattery {
    fn infobar_collapsed(&self) {
        self.imp()
            .infobar_content
            .get()
            .and_then(|ic| Some(ic.set_margin_top(10)));
    }

    fn infobar_uncollapsed(&self) {
        self.imp()
            .infobar_content
            .get()
            .and_then(|ic| Some(ic.set_margin_top(65)));
    }
}

impl PerformancePageBattery {
    pub fn new(name: &str, settings: &gio::Settings) -> Self {
        let this: Self = glib::Object::builder().property("name", name).build();

        fn update_refresh_rate_sensitive_labels(
            this: &PerformancePageBattery,
            settings: &gio::Settings,
        ) {
            let data_points = settings.int("performance-page-data-points") as u32;
            let delay = settings.uint64("app-update-interval-u64");
            let graph_max_duration =
                (((delay as f64) * INTERVAL_STEP) * (data_points as f64)).round() as u32;

            let this = this.imp();

            let time_string = &to_short_human_readable_time(graph_max_duration);

            this.energy_rate_max_duration.set_text(time_string);
        }
        update_refresh_rate_sensitive_labels(&this, settings);

        settings.connect_changed(Some("performance-page-data-points"), {
            let this = this.downgrade();
            move |settings, _| {
                if let Some(this) = this.upgrade() {
                    update_refresh_rate_sensitive_labels(&this, settings);
                }
            }
        });

        settings.connect_changed(Some("app-update-interval-u64"), {
            let this = this.downgrade();
            move |settings, _| {
                if let Some(this) = this.upgrade() {
                    update_refresh_rate_sensitive_labels(&this, settings);
                }
            }
        });

        let mut energy_rate_graph = DatasetGroup::new();
        energy_rate_graph.dataset_settings.scaling_settings = ScalingSettings::StickyUpDown;
        energy_rate_graph.dataset_settings.high_watermark = 45.;
        energy_rate_graph.dataset_settings.low_watermark = 35.;

        this.imp().energy_rate_graph.add_dataset(energy_rate_graph);

        this.imp().energy_rate_graph.connect_to_settings(settings);

        this
    }

    pub fn set_static_information(&self, battery_info: &Battery) -> bool {
        imp::PerformancePageBattery::set_static_information(self, battery_info)
    }

    pub fn update_readings(&self, battery_info: &Battery, index: Option<usize>) -> bool {
        imp::PerformancePageBattery::update_readings(self, battery_info, index)
    }

    pub fn update_animations(&self, new_ticks: f32) -> bool {
        imp::PerformancePageBattery::update_animations(self, new_ticks)
    }
}
