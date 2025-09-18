/* widgets/about_system_dialog.rs
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

use adw::subclass::prelude::*;
use gtk::glib::{self};
use gtk::prelude::{StaticTypeExt, WidgetExt};
use magpie_types::about::About;

mod imp {
    use super::*;
    use adw::PreferencesRow;
    use gtk::prelude::WidgetExt;
    use magpie_types::about::about::OsInfo;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/widgets/about_system_dialog.ui")]
    pub struct AboutSystemDialog {
        #[template_child]
        os_name: TemplateChild<gtk::Label>,
        #[template_child]
        version: TemplateChild<gtk::Label>,

        #[template_child]
        kernel_release: TemplateChild<gtk::Label>,
        #[template_child]
        kernel_version: TemplateChild<gtk::Label>,
    }

    impl Default for AboutSystemDialog {
        fn default() -> Self {
            Self {
                os_name: Default::default(),
                version: Default::default(),
                kernel_release: Default::default(),
                kernel_version: Default::default(),
            }
        }
    }

    impl AboutSystemDialog {
        fn bind_text(label: &TemplateChild<gtk::Label>, text: Option<String>) -> bool {
            if let Some(text) = text {
                label.set_text(&text);
                label.set_visible(true);

                true
            } else {
                label.set_visible(false);

                false
            }
        }

        fn format_kernel_release_string(os_info: &OsInfo) -> Option<String> {
            match (os_info.os_type.clone(), os_info.kernel_release.clone()) {
                (Some(kernel), Some(release)) => Some(format!("{kernel} {release}")),
                (None, Some(release)) => Some(format!("Unknown {release}")),
                (Some(kernel), None) => Some(kernel),
                (None, None) => None,
            }
        }

        pub fn setup(&self, about: About) {
            let os_info = about.os_info;

            let _ = Self::bind_text(&self.os_name, os_info.pretty_name.clone())
                || Self::bind_text(&self.os_name, os_info.name.clone());
            let _ = Self::bind_text(&self.version, os_info.version_id.clone())
                || Self::bind_text(&self.os_name, os_info.version.clone());

            let _ = Self::bind_text(
                &self.kernel_release,
                Self::format_kernel_release_string(&os_info),
            );
            let _ = Self::bind_text(&self.kernel_version, os_info.kernel_version.clone());
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AboutSystemDialog {
        const NAME: &'static str = "AboutSystemDialog";
        type Type = super::AboutSystemDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AboutSystemDialog {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for AboutSystemDialog {
        fn realize(&self) {
            self.parent_realize();
        }
    }

    impl AdwDialogImpl for AboutSystemDialog {
        fn closed(&self) {}
    }
}

glib::wrapper! {
    pub struct AboutSystemDialog(ObjectSubclass<imp::AboutSystemDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AboutSystemDialog {
    pub fn new(about: About) -> Self {
        let this: Self = glib::Object::builder()
            .property("follows-content-size", true)
            .build();

        let imp = this.imp();

        imp.setup(about);

        this
    }
}
