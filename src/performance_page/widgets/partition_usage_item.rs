/* performance_page/widgets/partition_usage_item
 *
 * Copyright 2024 Mission Center Devs
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

use adw::glib::g_warning;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::Properties;
use gtk::glib::{self, g_critical, WeakRef};
use magpie_types::disks::PartitionInfo;
use std::cell::{Cell, RefCell};
use std::ops::Div;

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate, Default)]
    #[template(
        resource = "/io/missioncenter/MissionCenter/ui/performance_page/disk_widgets/disk_partition_usage_item.ui"
    )]
    pub struct PartitionUsageItem {
        #[template_child]
        pub devname_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub mountdir: TemplateChild<gtk::Label>,
        #[template_child]
        pub usage_pct: TemplateChild<gtk::Label>,
        #[template_child]
        pub usage_bar: TemplateChild<gtk::ProgressBar>,

        moundir_name: Cell<Option<String>>,

        pub devname: Cell<String>,
        size: Cell<u64>,
        used: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PartitionUsageItem {
        const NAME: &'static str = "PartitionUsageItem";
        type Type = super::PartitionUsageItem;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PartitionUsageItem {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for PartitionUsageItem {}

    impl BoxImpl for PartitionUsageItem {}

    impl ListBoxRowImpl for PartitionUsageItem {}
}

glib::wrapper! {
    pub struct PartitionUsageItem(ObjectSubclass<imp::PartitionUsageItem>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl PartitionUsageItem {
    pub fn new() -> Self {
        let this: Self = glib::Object::builder().build();

        this
    }

    pub fn from_part_info(info: &PartitionInfo) -> PartitionUsageItem {
        let out = Self::new();

        out.imp().devname.set(info.devname.clone());
        out.imp()
            .devname_label
            .set_text(&format!("/dev/{}", &info.devname));

        out.update(info);

        out
    }

    pub fn update(&self, info: &PartitionInfo) {
        let imp = self.imp();
        let mountdir = &imp.mountdir;

        if let Some(dir) = info.mountpoint.as_ref() {
            mountdir.set_label(dir)
        } else {
            mountdir.set_visible(false);
        }

        match (info.size, info.used) {
            (Some(size), Some(used)) => {
                imp.usage_bar
                    .set_fraction(((used as f64) / (size as f64)).clamp(0., 1.));
            }
            _ => {}
        }
    }
}
