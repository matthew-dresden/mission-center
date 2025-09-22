/* process_tree/util.rs
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

use adw::gdk;
use adw::glib::{g_critical, g_warning};
use gtk::prelude::{IsA, WidgetExt};
use gtk::Widget;

pub fn calculate_anchor_point(
    page: &impl IsA<Widget>,
    widget: &Option<gtk::Widget>,
    x: f64,
    y: f64,
) -> (gdk::Rectangle, bool) {
    let imp = page;

    let Some(anchor_widget) = widget else {
        g_warning!(
            "MissionCenter::ProcessTree",
            "Failed to get anchor widget, popup will display in an arbitrary location"
        );
        return (gdk::Rectangle::new(0, 0, 0, 0), false);
    };

    if x > 0. && y > 0. {
        (
            match anchor_widget.compute_point(imp, &gtk::graphene::Point::new(x as _, y as _)) {
                Some(p) => gdk::Rectangle::new(p.x().round() as i32, p.y().round() as i32, 1, 1),
                None => {
                    g_critical!(
                    "MissionCenter::ProcessTree",
                    "Failed to compute_point, context menu will not be anchored to mouse position"
                );
                    gdk::Rectangle::new(x.round() as i32, y.round() as i32, 1, 1)
                }
            },
            false,
        )
    } else {
        (
            if let Some(bounds) = anchor_widget.compute_bounds(&*imp) {
                gdk::Rectangle::new(
                    bounds.x() as i32,
                    bounds.y() as i32,
                    bounds.width() as i32,
                    bounds.height() as i32,
                )
            } else {
                g_warning!(
                "MissionCenter::ProcessTree",
                "Failed to get bounds for menu button, popup will display in an arbitrary location"
            );
                gdk::Rectangle::new(0, 0, 0, 0)
            },
            true,
        )
    }
}
