use crate::process_tree::column_view_frame::imp::ColumnViewFrame;
use crate::process_tree::column_view_frame::ColumnViewSettingsValues::*;
use crate::settings;
use glib::g_critical;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

pub fn configure_column_frame(imp: &ColumnViewFrame) {
    let obj = imp.obj();

    let settings = settings!();

    settings
        .bind(
            "apps-page-show-column-separators",
            &*obj,
            "show-column-separators",
        )
        .build();

    imp.use_merged_stats
        .set(settings.boolean("apps-page-merged-process-stats"));
    settings.connect_changed(Some("apps-page-merged-process-stats"), {
        let this = obj.downgrade();
        move |settings, _| {
            if let Some(this) = this.upgrade() {
                this.imp()
                    .use_merged_stats
                    .set(settings.boolean("apps-page-merged-process-stats"));
            }
        }
    });

    configure_sorting(imp, &settings!());
}

fn configure_sorting(column_view_frame: &ColumnViewFrame, settings: &gio::Settings) {
    let column_view = &column_view_frame.column_view;

    let sorting_column_name_key = &column_view_frame.format_settings_key(&SortingColumnName);
    let sorting_order_key = &column_view_frame.format_settings_key(&SortingOrder);

    if !settings.boolean("apps-page-remember-sorting") {
        let _ = settings.set_string(sorting_column_name_key, "");
        let _ = settings.set_enum(sorting_order_key, gtk::ffi::GTK_SORT_ASCENDING);
        return;
    }

    let saved_id = settings.string(sorting_column_name_key);
    let order = settings.enum_(sorting_order_key);

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
                "MissionCenter::ProcessTree",
                "Unknown column sorting order retrieved from settings, sorting in ascending order as a fallback"
            );
            gtk::SortType::Ascending
        }
    };
    column_view.sort_by_column(matched_column.as_ref(), order);
}
