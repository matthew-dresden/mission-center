use crate::process_tree::column_view_frame::imp::ColumnViewFrame;
use crate::settings;
use glib::g_critical;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

pub fn configure_column_frame(imp: &ColumnViewFrame) {
    let neo_services_page = imp.obj();

    let settings = settings!();

    settings
        .bind(
            "apps-page-show-column-separators",
            &*neo_services_page,
            "show-column-separators",
        )
        .build();

    imp.use_merged_stats
        .set(settings.boolean("apps-page-merged-process-stats"));
    settings.connect_changed(Some("apps-page-merged-process-stats"), {
        let this = neo_services_page.downgrade();
        move |settings, _| {
            if let Some(this) = this.upgrade() {
                this.imp()
                    .use_merged_stats
                    .set(settings.boolean("apps-page-merged-process-stats"));
            }
        }
    });

    configure_sorting(&imp.column_view, &settings!());
}

fn configure_sorting(column_view: &gtk::ColumnView, settings: &gio::Settings) {
    if !settings.boolean("apps-page-remember-sorting") {
        let _ = settings.set_string("apps-page-sorting-column-name", "");
        let _ = settings.set_enum("apps-page-sorting-order", gtk::ffi::GTK_SORT_ASCENDING);
        return;
    }

    let saved_id = settings.string("apps-page-sorting-column-name");
    let order = settings.enum_("apps-page-sorting-order");

    let columns = column_view.columns();
    let mut matched_column = None;
    for i in 0..columns.n_items() {
        let Some(column) = columns
            .item(i)
            .and_then(|i| i.downcast::<gtk::ColumnViewColumn>().ok())
        else {
            continue;
        };
        let Some(column_id) = column.id() else {
            continue;
        };

        if column_id == saved_id.as_str() {
            matched_column = Some(column);
            break;
        }
    }

    let order = match order {
        gtk::ffi::GTK_SORT_ASCENDING => gtk::SortType::Ascending,
        gtk::ffi::GTK_SORT_DESCENDING => gtk::SortType::Descending,
        255 => return,
        _ => {
            g_critical!(
                "MissionCenter::ServicesPage",
                "Unknown column sorting order retrieved from settings, sorting in ascending order as a fallback"
            );
            gtk::SortType::Ascending
        }
    };
    column_view.sort_by_column(matched_column.as_ref(), order);
}
