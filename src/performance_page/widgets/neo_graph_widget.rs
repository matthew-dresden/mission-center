/* performance_page/widgets/graph_widget.rs
 *
 * Copyright 2025 Missioncenter Devs
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

use std::cell::Cell;
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use glib::{ParamSpec, Properties, Value};
use gtk::{gdk, gdk::prelude::*, gio, glib::{
    self,
    subclass::{prelude::*, Signal},
}, graphene, gsk::{self, FillRule, PathBuilder, Stroke}, prelude::*, subclass::prelude::*, Snapshot};

// pub use imp::DataSetDescriptor;

use super::{DatasetGroup, ScalingSettings, GRAPH_RADIUS};

mod imp {
    use super::*;
    use crate::performance_page::widgets::neo_graph_widget_utils::DatasetGroup;

    #[derive(Properties)]
    #[properties(wrapper_type = super::GraphWidgetNeo)]
    pub struct GraphWidgetNeo {
        #[property(get, set = Self::set_data_points)]
        data_points: Cell<u32>,
        #[property(get, set)]
        base_color: Cell<gdk::RGBA>,
        #[property(get, set = Self::set_horizontal_line_count)]
        horizontal_line_count: Cell<u32>,
        #[property(get, set = Self::set_vertical_line_count)]
        vertical_line_count: Cell<u32>,
        #[property(get, set)]
        animation_ticks: Cell<u32>,
        #[property(get, set = Self::set_expected_animation_ticks)]
        expected_animation_ticks: Cell<u32>,
        #[property(get, set = Self::set_do_animation)]
        do_animation: Cell<bool>,
        #[property(get, set = Self::set_smooth_graphs)]
        smooth_graphs: Cell<bool>,
        #[property(get, set)]
        scroll: Cell<bool>,
        #[property(get, set)]
        grid_visible: Cell<bool>,

        pub settings_inited: Cell<bool>,
        scroll_offset: Cell<u32>,
        prev_size: Cell<(i32, i32)>,

        base_snapshot: Cell<Option<Snapshot>>,
        cached_snapshot: Cell<Option<gsk::RenderNode>>,

        pub primary_dataset: Cell<Option<&'static DatasetGroup>>,
        pub data_sets: Cell<Vec<DatasetGroup>>,
    }

    impl Default for GraphWidgetNeo {
        fn default() -> Self {
            Self {
                data_points: Cell::new(0),
                base_color: Cell::new(gdk::RGBA::new(0., 0., 0., 1.)),
                horizontal_line_count: Cell::new(6),
                vertical_line_count: Cell::new(9),
                animation_ticks: Cell::new(0),
                expected_animation_ticks: Cell::new(0),
                do_animation: Cell::new(false),
                smooth_graphs: Cell::new(false),
                scroll: Cell::new(true),
                grid_visible: Cell::new(true),
                settings_inited: Cell::new(false),
                scroll_offset: Cell::new(0),
                prev_size: Cell::new((0, 0)),
                base_snapshot: Cell::new(None),
                cached_snapshot: Cell::new(None),
                primary_dataset: Cell::new(None),
                data_sets: Cell::new(vec![]),
            }
        }
    }

    impl GraphWidgetNeo {
        pub fn set_data_points(&self, count: u32) {
            if self.data_points.take() != count {
                let mut data_sets = self.data_sets.take();
                for values in data_sets.iter_mut() {
                    values.update_data_points(count as usize);
                }
                self.data_sets.set(data_sets);
            }
            self.data_points.set(count);
        }

        pub fn set_data_points_i32(&self, count: i32) {
            self.set_data_points(count as u32)
        }

        pub fn set_expected_animation_ticks_u64(&self, count: u64) {
            self.set_expected_animation_ticks(count as u32)
        }

        fn set_horizontal_line_count(&self, count: u32) {
            if self.horizontal_line_count.get() != count {
                self.horizontal_line_count.set(count);
                self.obj().upcast_ref::<super::GraphWidgetNeo>().queue_draw();
            }
        }

        fn set_vertical_line_count(&self, count: u32) {
            if self.vertical_line_count.get() != count {
                self.vertical_line_count.set(count);
                self.obj().upcast_ref::<super::GraphWidgetNeo>().queue_draw();
            }
        }

        pub fn set_smooth_graphs(&self, smooth: bool) {
            if self.smooth_graphs.get() != smooth {
                self.smooth_graphs.set(smooth);
                self.obj().upcast_ref::<super::GraphWidgetNeo>().queue_draw();
            }
        }

        pub fn set_do_animation(&self, smooth: bool) {
            if self.do_animation.get() != smooth {
                self.do_animation.set(smooth);
                self.obj().upcast_ref::<super::GraphWidgetNeo>().queue_draw();
            }
        }

        pub fn set_expected_animation_ticks(&self, ticks: u32) {
            if ticks > 0 {
                // let ticks = (((ticks as f64) * INTERVAL_STEP) * (self.data_points.get() as f64)).round() as u32;
                if self.expected_animation_ticks.get() != ticks {
                    self.expected_animation_ticks.set(ticks);
                }
            }
        }

        pub fn try_increment_scroll(&self) {
            if !self.scroll.get() {
                return;
            }

            self.scroll_offset
                .set(self.scroll_offset.get().wrapping_add(1));
        }
    }

    impl GraphWidgetNeo {
        #[inline]
        fn draw_outline(&self, snapshot: &Snapshot, bounds: &gsk::RoundedRect, color: &gdk::RGBA) {
            let stroke_color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 1.);
            snapshot.append_border(&bounds, &[1.; 4], &[stroke_color.clone(); 4]);
        }

        #[inline]
        fn draw_grid(
            &self,
            snapshot: &Snapshot,
            width: f32,
            height: f32,
            scale_factor: f64,
            data_point_count: usize,
            color: &gdk::RGBA,
        ) {
            let scale_factor = scale_factor as f32;
            let color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 51. / 256.);

            let stroke = Stroke::new(1.);

            // Draw horizontal lines
            let horizontal_line_count = self.obj().horizontal_line_count() + 1;

            let col_width = width - scale_factor;
            let col_height = height / horizontal_line_count as f32;

            for i in 1..horizontal_line_count {
                let path_builder = PathBuilder::new();
                path_builder.move_to(scale_factor / 2., col_height * i as f32);
                path_builder.line_to(col_width, col_height * i as f32);
                snapshot.append_stroke(&path_builder.to_path(), &stroke, &color);
            }

            // Draw vertical lines
            let vertical_line_count = self.obj().vertical_line_count() + 1;

            let animdist = if self.do_animation.get() {
                width / (data_point_count - 2) as f32
            } else {
                width / (data_point_count - 1) as f32
            };

            let col_width = width / vertical_line_count as f32;
            let col_height = height - scale_factor;

            let anim_offset = if self.obj().scroll() {
                ((animdist)
                    * (-(self.scroll_offset.get() as f32) + 1f32
                        - self.animation_ticks.get().saturating_sub(1) as f32
                            / self.expected_animation_ticks.get() as f32))
                    .rem_euclid(col_width)
            } else {
                0.
            };

            for i in 0..vertical_line_count {
                let path_builder = PathBuilder::new();
                path_builder.move_to(col_width * i as f32 + anim_offset, scale_factor / 2.);
                path_builder.line_to(col_width * i as f32 + anim_offset, col_height);
                snapshot.append_stroke(&path_builder.to_path(), &stroke, &color);
            }
        }

        fn render(&self, snapshot: &Snapshot, width: f32, height: f32, scale_factor: f64) {
            let base_color = self.base_color.get();

            let radius = graphene::Size::new(GRAPH_RADIUS, GRAPH_RADIUS);
            let bounds = gsk::RoundedRect::new(
                graphene::Rect::new(0., 0., width, height),
                radius,
                radius,
                radius,
                radius,
            );

            // probably also add a force redraw
            let mut cached_shot = self.cached_snapshot.take();

            if self.animation_ticks.get() <= 1 {
                self.try_increment_scroll();
            }

            let need_redraw = !self.do_animation.get() || self.animation_ticks.get() <= 1 || cached_shot.is_none();

            if need_redraw {
                let beanshot = Snapshot::new();
                let snapshot = &beanshot;

                if self.obj().grid_visible() {
                self.draw_grid(
                    snapshot,
                    width,
                    height,
                    scale_factor,
                    self.obj().data_points() as _,
                    &base_color,
                );
                }

                let mut data_sets = self.data_sets.take();
                let object = self.obj();
                for values in &mut data_sets {
                    values.plot(snapshot, width, height, scale_factor, &*object);
                }
                self.data_sets.set(data_sets);

                cached_shot = beanshot.to_node();
            }

            let Some(baze) = cached_shot else {
                println!("Oh no! No cached render when it was expected");
                return ;
            };

            snapshot.push_rounded_clip(&bounds);

            if self.do_animation.get() {
                let spacing = width / (self.data_points.get() - 2) as f32;
                snapshot.translate(&graphene::Point::new(spacing * ((self.expected_animation_ticks.get() - self.animation_ticks.get()) as f32 / self.expected_animation_ticks.get() as f32), 0.));
            }

            snapshot.append_node(baze.clone());

            if self.do_animation.get() {
                let spacing = width / (self.data_points.get() - 2) as f32;
                snapshot.translate(&graphene::Point::new(-spacing * ((self.expected_animation_ticks.get() - self.animation_ticks.get()) as f32 / self.expected_animation_ticks.get() as f32), 0.));
            }

            self.cached_snapshot.set(Some(baze));

            snapshot.pop();

            self.draw_outline(snapshot, &bounds, &base_color);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphWidgetNeo {
        const NAME: &'static str = "GraphWidgetNeo";
        type Type = super::GraphWidgetNeo;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for GraphWidgetNeo {
        fn properties() -> &'static [ParamSpec] {
            Self::derived_properties()
        }

        fn signals() -> &'static [Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("resize").build()])
        }

        fn set_property(&self, id: usize, value: &Value, pspec: &ParamSpec) {
            self.derived_set_property(id, value, pspec);
        }

        fn property(&self, id: usize, pspec: &ParamSpec) -> Value {
            self.derived_property(id, pspec)
        }
    }

    impl WidgetImpl for GraphWidgetNeo {
        fn realize(&self) {
            self.parent_realize();
        }

        fn snapshot(&self, snapshot: &Snapshot) {
            use glib::g_critical;

            let this = self.obj();

            let native = match this.native() {
                Some(native) => native,
                None => {
                    g_critical!("MissionCenter::GraphWidget", "Failed to get native");
                    return;
                }
            };

            let surface = match native.surface() {
                Some(surface) => surface,
                None => {
                    g_critical!("MissionCenter::GraphWidget", "Failed to get surface");
                    return;
                }
            };

            let (prev_width, prev_height) = self.prev_size.get();
            let (width, height) = (this.width(), this.height());

            if prev_width != width || prev_height != height {
                this.emit_by_name::<()>("resize", &[]);
                self.prev_size.set((width, height));
            }

            self.render(snapshot, width as f32, height as f32, surface.scale());
        }
    }
}

macro_rules! connect_setting {
    ($self: ident, $settings: ident, $setting_key: literal, $settings_method: ident, $set_fn: ident) => {
        $settings.connect_changed(Some($setting_key), {
            let this = $self.downgrade();

            move |settings, _| {
                if let Some(this) = this.upgrade() {
                    this.imp().$set_fn(settings.$settings_method($setting_key));
                }
            }
        });

        $self.imp().$set_fn($settings.$settings_method($setting_key));
    };
}

glib::wrapper! {
    pub struct GraphWidgetNeo(ObjectSubclass<imp::GraphWidgetNeo>)
        @extends gtk::Widget,
        @implements gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl GraphWidgetNeo {
    pub fn new(settings: Option<&gio::Settings>) -> Self {
        let obj: Self = glib::Object::new();

        {
            let this = obj.imp();

            if let Some(settings) = settings {
                obj.connect_to_settings(settings);
            }

            obj.connect_local("resize", true, {
                let dcr = obj.downgrade();

                move |_| {
                    let Some(obj) = dcr.upgrade() else { return None };

                    let width = obj.width() as f32;
                    let height = obj.height() as f32;

                    let (a, b) = if width > height {
                        (height, width)
                    } else {
                        (width, height)
                    };

                    obj.set_vertical_line_count((width * (a / b) / 30.).round().max(5.) as u32);

                    None
                }
            });
        }

        obj
    }

    pub fn set_dashed(&self, index: usize, dashed: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].dataset_settings.dashed = dashed;
        }
        self.imp().data_sets.set(data);
    }

    pub fn set_filled(&self, index: usize, filled: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].dataset_settings.fill = filled;
        }
        self.imp().data_sets.set(data);
    }

    pub fn set_data_visible(&self, index: usize, visible: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].dataset_settings.visible = visible;
        }
        self.imp().data_sets.set(data);
    }

    pub fn add_data_point(&self, datas: Vec<Vec<f32>>) {
        let this = self.imp();
        let mut data = this.data_sets.take();

        self.set_animation_ticks(0);

        if !this.settings_inited.get() {
            let _ = 1 + 1;
        }

        assert_eq!(datas.len(), data.len());

        for (idx, dataset) in data.iter_mut().enumerate() {
            dataset.add_data(&datas[idx]);
        }

        Self::apply_followings(&mut data);

        this.data_sets.set(data);
    }

    pub fn add_single_data_point(&self, idx: usize, datas: Vec<f32>) {
        let mut data = self.imp().data_sets.take();

        self.set_animation_ticks(0);

        data[idx].add_data(&datas);

        Self::apply_followings(&mut data);

        self.imp().data_sets.set(data);
    }

    fn apply_followings(mut data: &mut Vec<DatasetGroup>) {
        loop {
            let mut breaking = true;

            for i in 0..data.len() {
                let dataset = &data[i];

                let Some(other) = dataset.dataset_settings.following.map(|it| data[it].clone()) else {
                    continue;
                };

                breaking &= !data[i].apply_following_rules(Some(&other));
            }

            if breaking { break; }
        }
    }

    pub fn update_animation(&self) -> bool {
        if self.is_visible() {
            if self.do_animation() {
                self.set_animation_ticks(
                    (self.animation_ticks() + 1).min(self.expected_animation_ticks()),
                );
            } else if self.animation_ticks() == 0 {
                self.set_animation_ticks(self.expected_animation_ticks());
            } else {
                return true;
            }

            self.queue_draw();
        }

        true
    }

    pub fn add_dataset(&self, mut dataset: DatasetGroup) {
        let mut sets = self.imp().data_sets.take();

        dataset.update_data_points(self.data_points() as usize);
        sets.push(dataset);

        self.imp().data_sets.set(sets);
    }

    pub fn set_dataset_scaling(&self, index: usize, scaler: ScalingSettings) {
        let mut sets = self.imp().data_sets.take();

        let it = sets[index].dataset_settings.scaling_settings = scaler;

        self.imp().data_sets.set(sets);
    }

    pub fn set_all_datasets_scaling(&self, scaler: ScalingSettings) {
        let mut sets = self.imp().data_sets.take();

        for set in sets.iter_mut() {
            set.dataset_settings.scaling_settings = scaler.clone();
        }

        self.imp().data_sets.set(sets);
    }

    pub fn set_all_datasets_max_scale(&self, max: f32) {
        let mut sets = self.imp().data_sets.take();

        for set in sets.iter_mut() {
            set.dataset_settings.high_watermark = max;
        }

        self.imp().data_sets.set(sets);
    }

    pub fn set_dataset_max_scale(&self, index: usize, max: f32) {
        let mut sets = self.imp().data_sets.take();

        assert!(index < sets.len());

        sets[index].dataset_settings.high_watermark = max;

        self.imp().data_sets.set(sets);
    }

    pub fn get_dataset_max_scale(&self, index: usize) -> f32 {
        let mut sets = self.imp().data_sets.take();

        let it = sets[index].dataset_settings.high_watermark;

        self.imp().data_sets.set(sets);

        it
    }

    pub fn connect_to_settings(&self, settings: &gio::Settings) {
        // create a lock to prevent re-connectiong?
        self.imp().settings_inited.set(true);

        connect_setting!(self, settings, "performance-page-data-points", int, set_data_points_i32);
        connect_setting!(self, settings, "performance-smooth-graphs", boolean, set_smooth_graphs);
        connect_setting!(self, settings, "performance-sliding-graphs", boolean, set_do_animation);
        connect_setting!(self, settings, "app-update-interval-u64", uint64, set_expected_animation_ticks_u64);
    }

    pub fn connect_datasets(&self, idxa: usize, idxb: usize) {
        let this = self.imp();

        let mut sets = this.data_sets.take();

        sets[idxa].dataset_settings.following = Some(idxb);
        sets[idxb].dataset_settings.followed = Some(idxa);

        this.data_sets.set(sets);
    }
}

