/* performance_page/widgets/graph_widget.rs
 *
 * Copyright 2024 Romeo Calota
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

use glib::{ParamSpec, Properties, Value};
use gtk::{
    gdk,
    gdk::prelude::*,
    glib::{
        self,
        subclass::{prelude::*, Signal},
    },
    graphene,
    gsk::{self, FillRule, PathBuilder, Stroke},
    prelude::*,
    subclass::prelude::*,
    Snapshot,
};

pub use imp::DataSetDescriptor;

use super::GRAPH_RADIUS;

/// Values are truncated to the minimum and maximum values
const NO_SCALING: i32 = 0;
/// The graph min and max values are adjusted to the incoming data
const AUTO_SCALING: i32 = 1;
/// The graph min and max values are adjusted to the next power of 2
const AUTO_POW2_SCALING: i32 = 2;
/// The graph min and max values are hardcoded to the range [0, 1], and all values are normalized to this range
const NORMALIZED_SCALING: i32 = 3;

mod imp {
    use super::*;

    #[derive(Clone)]
    pub struct DataSetDescriptor {
        pub dashed: bool,
        pub fill: bool,
        pub visible: bool,

        pub data_set: Vec<f32>,
        pub max_all_time: f32,
    }

    #[derive(Properties)]
    #[properties(wrapper_type = super::GraphWidget)]
    pub struct GraphWidget {
        #[property(get, set = Self::set_data_points)]
        data_points: Cell<u32>,
        #[property(get, set = Self::set_data_sets)]
        data_set_count: Cell<u32>,
        #[property(get, set = Self::set_value_range_min)]
        value_range_min: Cell<f32>,
        #[property(get, set = Self::set_value_range_max)]
        value_range_max: Cell<f32>,
        #[property(get, set = Self::set_scaling)]
        scaling: Cell<i32>,
        #[property(get, set)]
        only_scale_up: Cell<bool>,
        #[property(get, set)]
        minimum_upper_bound: Cell<f32>,
        #[property(get, set)]
        grid_visible: Cell<bool>,
        #[property(get, set)]
        scroll: Cell<bool>,
        #[property(get, set = Self::set_smooth_graphs)]
        smooth_graphs: Cell<bool>,
        #[property(get, set)]
        base_color: Cell<gdk::RGBA>,
        #[property(get, set = Self::set_horizontal_line_count)]
        horizontal_line_count: Cell<u32>,
        #[property(get, set = Self::set_vertical_line_count)]
        vertical_line_count: Cell<u32>,

        pub data_sets: Cell<Vec<DataSetDescriptor>>,

        scroll_offset: Cell<u32>,
        prev_size: Cell<(i32, i32)>,

        #[property(get, set)]
        animation_ticks: Cell<u32>,
        #[property(get, set = Self::set_expected_animation_ticks)]
        expected_animation_ticks: Cell<u32>,
        #[property(get, set)]
        do_animation: Cell<bool>,
    }

    impl Default for GraphWidget {
        fn default() -> Self {
            const DATA_SET_LEN_DEFAULT: usize = 60;

            let mut data_set = vec![0.0; DATA_SET_LEN_DEFAULT];
            data_set.reserve(1);

            Self {
                data_points: Cell::new(DATA_SET_LEN_DEFAULT as _),
                data_set_count: Cell::new(1),
                value_range_min: Cell::new(0.),
                value_range_max: Cell::new(100.),
                scaling: Cell::new(NO_SCALING),
                only_scale_up: Cell::new(false),
                minimum_upper_bound: Cell::new(f32::NAN),
                grid_visible: Cell::new(true),
                scroll: Cell::new(false),
                smooth_graphs: Cell::new(false),
                base_color: Cell::new(gdk::RGBA::new(0., 0., 0., 1.)),
                horizontal_line_count: Cell::new(9),
                vertical_line_count: Cell::new(6),

                data_sets: Cell::new(vec![DataSetDescriptor {
                    dashed: false,
                    fill: true,
                    visible: true,

                    data_set,
                    max_all_time: 0.,
                }]),

                scroll_offset: Cell::new(0),
                prev_size: Cell::new((0, 0)),
                animation_ticks: Cell::new(0),
                expected_animation_ticks: Cell::new(10),
                do_animation: Cell::new(false),
            }
        }
    }

    impl GraphWidget {
        fn set_value_range_min(&self, min: f32) {
            if self.scaling.get() == NORMALIZED_SCALING {
                self.value_range_min.set(0.);
                return;
            }

            self.value_range_min.set(min);
        }

        fn set_value_range_max(&self, max: f32) {
            if self.scaling.get() == NORMALIZED_SCALING {
                self.value_range_max.set(1.);
                return;
            }

            self.value_range_max.set(max);
        }

        fn set_scaling(&self, scaling: i32) {
            match scaling {
                NO_SCALING => {
                    self.scaling.set(NO_SCALING);
                    let mut data_sets = self.data_sets.take();

                    for values in data_sets.iter_mut() {
                        for value in values.data_set.iter_mut() {
                            *value =
                                value.clamp(self.value_range_min.get(), self.value_range_max.get());
                        }
                    }

                    self.data_sets.set(data_sets);
                }
                AUTO_SCALING..=AUTO_POW2_SCALING => {
                    self.scaling.set(scaling);

                    let mut data_sets = self.data_sets.take();

                    for values in data_sets.iter_mut() {
                        for value in values.data_set.iter_mut() {
                            *value = value.max(self.value_range_min.get());
                        }
                    }

                    self.data_sets.set(data_sets);
                }
                NORMALIZED_SCALING => {
                    self.value_range_min.set(0.);
                    self.value_range_max.set(1.);
                    self.scaling.set(NORMALIZED_SCALING);
                }
                _ => self.scaling.set(NO_SCALING),
            }
        }

        fn set_data_points(&self, count: u32) {
            if self.data_points.take() != count {
                let mut data_points = self.data_sets.take();
                for values in data_points.iter_mut() {
                    if count == (values.data_set.len() as u32) {
                        continue;
                    }
                    // we need to truncate from the correct side
                    values.data_set.reverse();
                    values.data_set.resize(count as _, 0.);
                    values.data_set.reverse();
                }
                self.data_sets.set(data_points);
            }
            self.data_points.set(count);
        }

        fn set_data_sets(&self, count: u32) {
            let mut data_points = self.data_sets.take();
            data_points.resize(
                count as _,
                DataSetDescriptor {
                    dashed: false,
                    fill: true,
                    visible: true,

                    data_set: vec![0.; self.data_points.get() as _],
                    max_all_time: 0.,
                },
            );
            self.data_sets.set(data_points);

            self.data_set_count.set(count);
        }

        fn set_horizontal_line_count(&self, count: u32) {
            if self.horizontal_line_count.get() != count {
                self.horizontal_line_count.set(count);
                self.obj().upcast_ref::<super::GraphWidget>().queue_draw();
            }
        }

        fn set_vertical_line_count(&self, count: u32) {
            if self.vertical_line_count.get() != count {
                self.vertical_line_count.set(count);
                self.obj().upcast_ref::<super::GraphWidget>().queue_draw();
            }
        }

        fn set_smooth_graphs(&self, smooth: bool) {
            if self.smooth_graphs.get() != smooth {
                self.smooth_graphs.set(smooth);
                self.obj().upcast_ref::<super::GraphWidget>().queue_draw();
            }
        }

        fn set_expected_animation_ticks(&self, ticks: u32) {
            if ticks > 0 {
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

    impl GraphWidget {
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

        #[inline]
        fn plot_values(
            &self,
            snapshot: &Snapshot,
            width: f32,
            height: f32,
            scale_factor: f64,
            data_points: &mut DataSetDescriptor,
            color: &gdk::RGBA,
        ) {
            let scale_factor = scale_factor as f32;
            let stroke_color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 1.);
            let fill_color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 100. / 256.);

            let stroke = Stroke::new(1.);

            let val_max = self.value_range_max.get() - self.value_range_min.get();
            let val_min = 0.;

            let spacing_x = if self.do_animation.get() {
                width / (data_points.data_set.len() - 2) as f32
            } else {
                width / (data_points.data_set.len() - 1) as f32
            };

            let mut points: Vec<(f32, f32)> = if self.scaling.get() != NORMALIZED_SCALING {
                (0..)
                    .map(|x| x as f32)
                    .zip(
                        data_points
                            .data_set
                            .iter()
                            .map(|x| *x - self.value_range_min.get()),
                    )
                    .skip_while(|(_, y)| *y <= scale_factor)
                    .collect()
            } else {
                let mut min = self.value_range_min.get();
                let mut max = self.value_range_max.get();
                for value in data_points.data_set.iter() {
                    if *value < min {
                        min = *value;
                    }
                    if *value > max {
                        max = *value;
                    }
                }

                if data_points.max_all_time < max {
                    data_points.max_all_time = max;
                }

                if self.only_scale_up.get() {
                    max = data_points.max_all_time;
                }

                (0..)
                    .map(|x| x as f32)
                    .zip(data_points.data_set.iter().map(|x| {
                        let downscale_factor = max - min;
                        if downscale_factor == 0. {
                            0.
                        } else {
                            (*x - min) / downscale_factor
                        }
                    }))
                    .collect()
            };

            for (x, y) in &mut points {
                *x = *x * spacing_x;
                if self.do_animation.get() {
                    *x -= spacing_x;
                }
                *y = height - ((y.clamp(val_min, val_max) / val_max) * (height));
            }

            if !points.is_empty() {
                let anim_offset = if self.do_animation.get() {
                    -spacing_x
                        * (1f32
                            - self.animation_ticks.get().saturating_sub(1) as f32
                                / self.expected_animation_ticks.get() as f32)
                } else {
                    0.
                };

                let startindex;
                let (mut x, mut y);
                let pointlen = points.len();

                if pointlen < data_points.data_set.len() {
                    (x, y) = (
                        ((data_points.data_set.len() as f32) - (pointlen + 2) as f32) * spacing_x,
                        height,
                    );
                    startindex = 0;
                } else {
                    (x, y) = points[0];
                    startindex = 1;
                }

                x -= anim_offset;

                let path_builder = PathBuilder::new();
                path_builder.move_to(x, y);

                let smooth = self.smooth_graphs.get();

                for i in startindex..pointlen {
                    (x, y) = points[i];

                    x -= anim_offset;
                    if smooth {
                        let (mut lastx, lasty);
                        if i > 0 {
                            (lastx, lasty) = points[i - 1];
                        } else {
                            (lastx, lasty) = (x - spacing_x + anim_offset, height);
                        }

                        lastx -= anim_offset;

                        path_builder.cubic_to(
                            lastx + spacing_x / 2f32,
                            lasty,
                            lastx + spacing_x / 2f32,
                            y,
                            x,
                            y,
                        );
                    } else {
                        path_builder.line_to(x, y);
                    }
                }

                // Make sure to close out the path
                path_builder.line_to(points[pointlen - 1].0, height);
                path_builder.line_to(points[0].0, height);
                path_builder.close();

                let path = path_builder.to_path();

                if data_points.fill {
                    snapshot.append_fill(&path, FillRule::Winding, &fill_color);
                }

                if data_points.dashed {
                    stroke.set_dash(&[5., 5.]);
                }

                snapshot.append_stroke(&path, &stroke, &stroke_color);
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

            snapshot.push_rounded_clip(&bounds);

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
            for values in &mut data_sets {
                if !values.visible {
                    continue;
                }

                self.plot_values(snapshot, width, height, scale_factor, values, &base_color);
            }
            self.data_sets.set(data_sets);

            snapshot.pop();

            self.draw_outline(snapshot, &bounds, &base_color);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphWidget {
        const NAME: &'static str = "GraphWidget";
        type Type = super::GraphWidget;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for GraphWidget {
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

    impl WidgetImpl for GraphWidget {
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

glib::wrapper! {
    pub struct GraphWidget(ObjectSubclass<imp::GraphWidget>)
        @extends gtk::Widget,
        @implements gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl GraphWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_dashed(&self, index: usize, dashed: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].dashed = dashed;
        }
        self.imp().data_sets.set(data);
    }

    pub fn set_filled(&self, index: usize, filled: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].fill = filled;
        }
        self.imp().data_sets.set(data);
    }

    pub fn set_data_visible(&self, index: usize, visible: bool) {
        let mut data = self.imp().data_sets.take();
        if index < data.len() {
            data[index].visible = visible;
        }
        self.imp().data_sets.set(data);
    }

    pub fn add_data_point(&self, index: usize, mut value: f32) {
        let mut data = self.imp().data_sets.take();

        self.set_animation_ticks(0);

        if index == 0 {
            self.imp().try_increment_scroll();
        }

        if index >= data.len() {
            self.imp().data_sets.set(data);
            return;
        }

        if value.is_infinite() || value.is_nan() {
            value = self.value_range_min();
        }

        if value.is_subnormal() {
            value = 0.;
        }

        if self.scaling() == NO_SCALING {
            value = value.clamp(self.value_range_min(), self.value_range_max());
        } else if self.scaling() == AUTO_SCALING || self.scaling() == AUTO_POW2_SCALING {
            value = value.max(self.value_range_min());
        }

        data[index].data_set.push(value);
        data[index].data_set.remove(0);

        if self.scaling() == AUTO_SCALING || self.scaling() == AUTO_POW2_SCALING {
            self.scale(&mut data, value);
        }

        self.imp().data_sets.set(data);
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

    pub fn data(&self, index: usize) -> Option<Vec<f32>> {
        let imp = self.imp();

        let data = imp.data_sets.take();
        let result = if index < data.len() {
            Some(data[index].data_set.clone())
        } else {
            None
        };
        imp.data_sets.set(data);

        result
    }

    pub fn set_data(&self, index: usize, mut values: Vec<f32>) {
        let imp = self.imp();
        let mut data = imp.data_sets.take();

        if index < data.len() {
            values.truncate(data[index].data_set.len());
            data[index].data_set = values;

            for x in &mut data[index].data_set {
                if x.is_infinite() || x.is_nan() {
                    *x = self.value_range_min();
                }

                if x.is_subnormal() {
                    *x = 0.;
                }

                if self.scaling() == NO_SCALING {
                    *x = x.clamp(self.value_range_min(), self.value_range_max());
                } else if self.scaling() == AUTO_SCALING || self.scaling() == AUTO_POW2_SCALING {
                    *x = x.max(self.value_range_min());
                }
            }

            if self.scaling() == AUTO_SCALING || self.scaling() == AUTO_POW2_SCALING {
                if let Some(max) = data[index]
                    .data_set
                    .iter()
                    .map(|x| *x)
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
                {
                    self.scale(&mut data, max);
                }
            }
        }

        imp.data_sets.set(data);
    }

    pub fn max_all_time(&self, index: usize) -> Option<f32> {
        let imp = self.imp();

        let mut result = None;

        let data = imp.data_sets.take();
        if index < data.len() {
            result = Some(data[index].max_all_time);
        }
        imp.data_sets.set(data);

        result
    }

    fn scale(&self, data: &mut Vec<DataSetDescriptor>, value: f32) {
        fn round_up_to_next_power_of_two(num: u64) -> u64 {
            if num == 0 {
                return 0;
            }

            let mut n = num - 1;
            n |= n >> 1;
            n |= n >> 2;
            n |= n >> 4;
            n |= n >> 8;
            n |= n >> 16;

            n + 1
        }

        let min_value = self.value_range_min();
        let max_norm = self.value_range_max() - min_value;
        let value = value - min_value;

        let mut max_y = value.max(max_norm);

        let mut value_max = value;
        for data_set in data.iter() {
            for value in data_set.data_set.iter() {
                if value_max < (*value - min_value) {
                    value_max = *value;
                }
            }
        }

        while value_max < max_y {
            max_y /= 2.;
        }
        if value_max > max_y {
            max_y *= 2.;
        }

        max_y += min_value;
        if self.scaling() == AUTO_POW2_SCALING {
            max_y = max_y.round();
            if max_y > 0. {
                max_y = round_up_to_next_power_of_two(max_y as u64) as f32;
            }
        }

        if max_y > self.value_range_min() {
            if (max_y < self.value_range_max()) && self.only_scale_up() {
                return;
            }

            let minimum_upper_bound = self.minimum_upper_bound();
            if minimum_upper_bound.is_normal() && max_y < minimum_upper_bound {
                max_y = minimum_upper_bound;
            }

            self.set_value_range_max(max_y);
        }
    }
}

impl GraphWidget {
    #[inline]
    pub fn no_scaling() -> i32 {
        NO_SCALING
    }

    #[inline]
    pub fn auto_scaling() -> i32 {
        AUTO_SCALING
    }

    #[inline]
    pub fn auto_pow2_scaling() -> i32 {
        AUTO_POW2_SCALING
    }

    #[inline]
    pub fn normalized_scaling() -> i32 {
        NORMALIZED_SCALING
    }
}
