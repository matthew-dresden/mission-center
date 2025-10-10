/* performance_page/widgets/graph_widget_utils.rs
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

use crate::performance_page::widgets::GraphWidget;
use crate::{MAX_POINTS, MIN_POINTS};
use adw::gdk;
use gtk::gsk::{FillRule, PathBuilder, Stroke};
use gtk::prelude::SnapshotExt;
use gtk::Snapshot;
use std::cmp::PartialEq;

#[derive(Default, Clone, PartialEq)]
pub enum ScalingSettings {
    #[default]
    Fixed,
    ScaleUp,
    ScaleDown,
    ScaleUpDown,
    ScaleUpPow2,
    ScaleDownPow2,
    ScaleUpDownPow2,
    StickyUp,
    StickyDown,
    StickyUpDown,
    Stacking,
}

#[derive(Clone)]
pub struct DatasetSettings {
    pub dashed: bool,
    pub fill: bool,
    pub visible: bool,

    pub scaling_settings: ScalingSettings,
    pub low_watermark: f32,
    pub high_watermark: f32,

    pub following: Option<usize>,
    pub followed: Option<usize>,
}

#[derive(Clone)]
pub struct DatasetGroup {
    pub dataset_settings: DatasetSettings,

    pub datas: Vec<Dataset>,
}

impl DatasetGroup {
    pub fn new() -> Self {
        Self {
            dataset_settings: DatasetSettings {
                dashed: false,
                fill: true,
                visible: true,
                scaling_settings: Default::default(),
                low_watermark: 0.0,
                high_watermark: 100.0,
                following: None,
                followed: None,
            },
            datas: vec![Dataset::default()],
        }
    }
}

#[derive(Clone)]
pub struct Dataset {
    data: Vec<f32>,
    pub used_data: usize,
}

#[derive(Clone, Debug)]
pub struct DatasetPoints {
    x: f32,
    y: f32,
}

impl DatasetGroup {
    pub fn set_datasets(&mut self, sets: usize) {
        for _ in self.datas.len()..sets {
            self.datas.push(Dataset::default());
        }
    }

    pub fn add_data(&mut self, points: &Vec<f32>) {
        for (idx, set) in points.iter().enumerate() {
            self.update_single_scaling(idx, *set);
        }

        self.update_expensive_scaling();
    }

    fn round_up_to_next_power_of_two(num: f32) -> f32 {
        let num = num as u64;

        if num == 0 {
            return 0.;
        }

        let mut n = num - 1;
        n |= n >> 1;
        n |= n >> 2;
        n |= n >> 4;
        n |= n >> 8;
        n |= n >> 16;

        (n + 1) as f32
    }

    fn update_expensive_scaling(&mut self) {
        match self.dataset_settings.scaling_settings {
            ScalingSettings::ScaleUp => {
                self.dataset_settings.high_watermark = self.get_maximum();
            }
            ScalingSettings::ScaleDown => {
                self.dataset_settings.low_watermark = self.get_minimum();
            }
            ScalingSettings::ScaleUpDown => {
                self.dataset_settings.high_watermark = self.get_maximum();
                self.dataset_settings.low_watermark = self.get_minimum();
            }
            ScalingSettings::ScaleUpPow2 => {
                self.dataset_settings.high_watermark =
                    Self::round_up_to_next_power_of_two(self.get_maximum());
            }
            ScalingSettings::ScaleDownPow2 => {
                // todo
            }
            ScalingSettings::ScaleUpDownPow2 => {
                self.dataset_settings.high_watermark =
                    Self::round_up_to_next_power_of_two(self.get_maximum());
                // todo
            }
            ScalingSettings::StickyUp => {}
            ScalingSettings::StickyDown => {}
            ScalingSettings::StickyUpDown => {}
            ScalingSettings::Stacking => {}
            ScalingSettings::Fixed => {}
        }
    }

    pub fn apply_following_rules(&mut self, other: Option<&Self>) -> bool {
        let Some(other) = other else {
            return false;
        };

        let mut changed = false;
        if other.dataset_settings.high_watermark > self.dataset_settings.high_watermark {
            self.dataset_settings.high_watermark = other.dataset_settings.high_watermark;
            changed = true;
        }

        if other.dataset_settings.low_watermark < self.dataset_settings.low_watermark {
            self.dataset_settings.low_watermark = other.dataset_settings.low_watermark;
            changed = true;
        }

        changed
    }

    fn get_minimum(&mut self) -> f32 {
        self.datas
            .iter()
            .filter_map(|set| set.get_data_removed().iter().map(|f| *f).reduce(f32::min))
            .reduce(f32::min)
            .unwrap_or(self.dataset_settings.low_watermark)
    }

    fn get_maximum(&mut self) -> f32 {
        self.datas
            .iter()
            .filter_map(|set| set.get_data_removed().iter().map(|f| *f).reduce(f32::max))
            .reduce(f32::max)
            .unwrap_or(self.dataset_settings.high_watermark)
    }

    // do cheap updates whenever a new point is added
    fn update_single_scaling(&mut self, idx: usize, point: f32) {
        self.datas[idx].data.rotate_right(1);
        self.datas[idx].data[0] = point;

        // do scaling up
        match self.dataset_settings.scaling_settings {
            /* these require searching, wait for the expensive call */
            ScalingSettings::ScaleUp => {}
            ScalingSettings::ScaleDown => {}
            ScalingSettings::ScaleUpDown => {}
            ScalingSettings::ScaleUpPow2 => {}
            ScalingSettings::ScaleDownPow2 => {}
            ScalingSettings::ScaleUpDownPow2 => {}
            ScalingSettings::StickyUp => {
                if point > self.dataset_settings.high_watermark {
                    self.dataset_settings.high_watermark = point
                }
            }
            ScalingSettings::StickyDown => {
                if point < self.dataset_settings.low_watermark {
                    self.dataset_settings.low_watermark = point
                }
            }
            ScalingSettings::StickyUpDown => {
                if point > self.dataset_settings.high_watermark {
                    self.dataset_settings.high_watermark = point
                }

                if point < self.dataset_settings.low_watermark {
                    self.dataset_settings.low_watermark = point
                }
            }
            ScalingSettings::Stacking => {}
            ScalingSettings::Fixed => {}
        }
    }

    pub fn update_data_points(&mut self, new_points: usize) {
        self.datas
            .iter_mut()
            .for_each(|set| set.update_data_points(new_points));
    }

    pub fn plot(&self, snapshot: &Snapshot, width: f32, height: f32, parent: &GraphWidget) {
        if !self.dataset_settings.visible {
            return;
        }

        let mut dataset_points: Vec<Vec<DatasetPoints>> = self
            .datas
            .iter()
            .map(|pts| pts.plot(width, height, &self.dataset_settings, parent))
            .collect();

        let dataset_points = match self.dataset_settings.scaling_settings {
            ScalingSettings::Stacking => {
                // Stack the dataset points on one another
                let mut stacked_points: Vec<Vec<DatasetPoints>> =
                    dataset_points.drain(0..1).collect();

                for (series_index, series) in dataset_points.iter().enumerate() {
                    let stacked_series = &stacked_points[series_index];
                    assert_eq!(series.len(), stacked_series.len());

                    let mut newset = Vec::new();

                    for (point_index, point) in series.iter().enumerate() {
                        newset.push(DatasetPoints {
                            x: point.x,
                            y: stacked_series[point_index].y + point.y - height,
                        })
                    }

                    stacked_points.push(newset);
                }

                stacked_points
            }
            _ => dataset_points,
        };

        let color = parent.base_color();

        let stroke_color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 1.);
        let fill_color = gdk::RGBA::new(color.red(), color.green(), color.blue(), 100. / 256.);

        let stroke = Stroke::new(1.);

        if self.dataset_settings.dashed {
            stroke.set_dash(&[5., 5.]);
        }

        for (set_index, set) in dataset_points.iter().enumerate() {
            let path_builder = PathBuilder::new();

            let (Some(first_point), Some(last_point)) = (set.first(), set.last()) else {
                continue;
            };

            path_builder.move_to(first_point.x, first_point.y);

            let (mut lastx, mut lasty) = (first_point.x, first_point.y);

            for point in set.iter().skip(1) {
                if parent.smooth_graphs() {
                    let deltax = point.x - lastx;
                    path_builder.cubic_to(
                        lastx + deltax / 2f32,
                        lasty,
                        lastx + deltax / 2f32,
                        point.y,
                        point.x,
                        point.y,
                    );

                    lastx = point.x;
                    lasty = point.y;
                } else {
                    path_builder.line_to(point.x, point.y);
                }
            }

            path_builder.line_to(last_point.x, height);
            path_builder.line_to(first_point.x, height);
            path_builder.close();

            let path = path_builder.to_path();

            if self.dataset_settings.fill
                && (self.dataset_settings.scaling_settings != ScalingSettings::Stacking
                    || set_index == dataset_points.len() - 1)
            {
                snapshot.append_fill(&path, FillRule::Winding, &fill_color);
            }

            snapshot.append_stroke(&path, &stroke, &stroke_color);
        }
    }
}

impl Dataset {
    pub fn update_data_points(&mut self, new_points: usize) {
        self.used_data = new_points;
    }

    pub fn get_data(&self) -> Vec<f32> {
        self.data
            .iter()
            .take(self.used_data)
            .map(|v| v.clone())
            .collect()
    }

    pub fn get_data_removed(&self) -> Vec<f32> {
        self.data
            .iter()
            .take(self.used_data)
            .filter(|v| v.is_normal())
            .map(|v| v.clone())
            .collect()
    }

    pub fn get_data_sanitized(&self, low_watermark: f32) -> Vec<f32> {
        self.data
            .iter()
            .take(self.used_data)
            .map(|v| {
                if !v.is_normal() {
                    low_watermark
                } else {
                    v.clone()
                }
            })
            .collect()
    }

    pub fn plot(
        &self,
        width: f32,
        height: f32,
        settings: &DatasetSettings,
        parent: &GraphWidget,
    ) -> Vec<DatasetPoints> {
        let val_min = settings.low_watermark;
        let val_max = settings.high_watermark.max(val_min + 1.);

        let spacing = width * parent.point_spacing_factor();

        let points: Vec<_> = (0..)
            .map(|x| x as f32)
            .zip(
                self.get_data_sanitized(val_min)
                    .iter()
                    .map(|y| (*y - val_min) / (val_max - val_min)),
            )
            .map(|(x, y)| (width - x * spacing, (1. - y) * height))
            .map(|(x, y)| DatasetPoints { x, y })
            .collect();

        points
    }
}

impl Default for Dataset {
    fn default() -> Self {
        Self {
            data: vec![f32::NAN; MAX_POINTS as usize],
            used_data: MIN_POINTS as usize,
        }
    }
}
