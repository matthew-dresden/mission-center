/* models/processes.rs
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

use std::collections::{HashMap, HashSet};

use gtk::gio;
use gtk::glib::g_critical;
use gtk::prelude::*;

use magpie_types::apps::icon::Icon;
use magpie_types::apps::App;
use magpie_types::processes::{Process, ProcessUsageStats};
use magpie_types::services::Service;

use crate::process_tree::row_model::{ContentType, RowModel, RowModelBuilder, SectionType};

fn service_to_section_type(_service: &Service) -> SectionType {
    SectionType::SecondSection
    // use std::env;
    /*    if let Some(user) = service.user.as_ref() {
            // todo have magpie set user or not
            if env::var_os("USER")
                .map(|u| u.to_str().map(|u| u != user))
                .flatten()
                .unwrap_or(true)
                || user.is_empty()
            {
                SectionType::SecondSection
            } else {
                SectionType::FirstSection
            }
        } else {
            SectionType::SecondSection
        }
    */
}

fn get_service_icon(service: &Service) -> String {
    if service.running {
        "service-running".into()
    } else {
        if service.failed {
            "service-failed".into()
        } else if service.enabled {
            "service-stopped".into()
        } else {
            "service-disabled".into()
        }
    }
}

pub fn update_services(
    process_map: &HashMap<u32, Process>,
    services: &HashMap<String, Service>,
    list: &gio::ListStore,
    app_icons: &HashMap<u32, String>,
    icon: &str,
    use_merged_stats: bool,
    section_type: SectionType,
) {
    let mut to_remove = Vec::with_capacity(list.n_items() as _);
    for i in (0..list.n_items()).rev() {
        let Some(service) = list.item(i).and_then(|obj| obj.downcast::<RowModel>().ok()) else {
            to_remove.push(i);
            continue;
        };

        if services.contains_key(service.id().as_str()) {
            continue;
        }

        to_remove.push(i);
    }
    for i in &to_remove {
        list.remove(*i);
    }

    for (service_id, service) in services {
        let service_section_type = service_to_section_type(&service);

        if service_section_type != section_type {
            continue;
        }

        let row_model = if let Some(index) = list.find_with_equal_func(|obj| {
            let Some(row_model) = obj.downcast_ref::<RowModel>() else {
                return false;
            };

            if row_model.content_type() != ContentType::Service {
                return false;
            }

            row_model.id().as_str() == service_id
        }) {
            unsafe {
                list.item(index)
                    .and_then(|obj| obj.downcast().ok())
                    .unwrap_unchecked()
            }
        } else {
            let row_model = RowModelBuilder::new()
                .content_type(ContentType::Service)
                .section_type(service_section_type)
                .id(&service.id)
                .build();
            list.append(&row_model);
            row_model
        };

        row_model.set_id(service_id.as_str());
        row_model.set_icon(get_service_icon(&service));
        row_model.set_name(service.id.as_str());

        if let Some(process) = process_map.get(service.pid.as_ref().unwrap_or(&0)) {
            let usage_stats = process.merged_usage_stats(&process_map);

            let command_line = process.cmd.join(" ");

            row_model.set_command_line(command_line);

            set_stats(&row_model, &usage_stats);
        }

        set_service(&row_model, service);

        if let Some(pid) = service.pid {
            to_remove.clear();
            let app_children = row_model.children();
            for i in (0..app_children.n_items()).rev() {
                let Some(child) = app_children
                    .item(i)
                    .and_then(|obj| obj.downcast::<RowModel>().ok())
                else {
                    to_remove.push(i);
                    continue;
                };

                if pid == child.pid() {
                    continue;
                }

                to_remove.push(i);
            }
            for i in &to_remove {
                app_children.remove(*i);
            }

            update_service_children(
                process_map,
                &pid,
                &app_children,
                app_icons,
                icon,
                use_merged_stats,
                service_section_type,
                service,
            );
        } else {
            row_model.children().remove_all();
        }
    }
}

pub fn update_service_children(
    process_map: &HashMap<u32, Process>,
    pid: &u32,
    list: &gio::ListStore,
    app_icons: &HashMap<u32, String>,
    icon: &str,
    use_merged_stats: bool,
    section_type: SectionType,
    parent_service: &Service,
) {
    let Some(process) = process_map.get(&pid) else {
        return;
    };

    let pretty_name = if process.exe.is_empty() {
        if let Some(cmd) = process.cmd.first() {
            let mut cmd = cmd
                .split_ascii_whitespace()
                .next()
                .and_then(|s| s.split('/').last())
                .unwrap_or(&process.name);
            if let Some(s) = cmd.strip_suffix(':') {
                cmd = s;
            }
            cmd.trim()
        } else {
            process.name.trim()
        }
    } else {
        let exe_name = process.exe.split('/').last().unwrap_or(&process.name);
        if exe_name.starts_with("wine") {
            if process.cmd.is_empty() {
                process.name.trim()
            } else {
                process.cmd[0]
                    .split("\\")
                    .last()
                    .unwrap_or(&process.name)
                    .split("/")
                    .last()
                    .unwrap_or(&process.name)
                    .trim()
            }
        } else {
            exe_name.trim()
        }
    };

    let row_model = if let Some(index) = list.find_with_equal_func(|obj| {
        let Some(row_model) = obj.downcast_ref::<RowModel>() else {
            return false;
        };
        row_model.pid() == process.pid
    }) {
        unsafe {
            list.item(index)
                .and_then(|obj| obj.downcast().ok())
                .unwrap_unchecked()
        }
    } else {
        let row_model = RowModelBuilder::new()
            .content_type(ContentType::Process)
            .section_type(section_type)
            .id(&process.pid.to_string())
            .build();
        list.append(&row_model);
        row_model
    };

    let prev_children = row_model.children();
    let mut to_remove = Vec::with_capacity(prev_children.n_items() as _);
    for i in (0..prev_children.n_items()).rev() {
        let Some(child) = prev_children
            .item(i)
            .and_then(|obj| obj.downcast::<RowModel>().ok())
        else {
            to_remove.push(i);
            continue;
        };

        if process.children.contains(&child.pid()) {
            continue;
        }

        to_remove.push(i);
    }
    for i in to_remove {
        prev_children.remove(i);
    }

    let merged_usage_stats = process.merged_usage_stats(&process_map);
    let usage_stats = if use_merged_stats {
        &merged_usage_stats
    } else {
        &process.usage_stats
    };

    let icon = if let Some(icon) = app_icons.get(&process.pid) {
        icon.as_str()
    } else {
        icon
    };

    let command_line = process.cmd.join(" ");

    row_model.set_name(pretty_name);
    // row_model.set_icon(icon);
    row_model.set_command_line(command_line);
    row_model.set_pid(process.pid);
    set_stats(&row_model, usage_stats);
    set_service(&row_model, parent_service);

    for child in &process.children {
        update_service_children(
            process_map,
            child,
            &row_model.children(),
            app_icons,
            icon,
            use_merged_stats,
            SectionType::SecondSection,
            parent_service,
        );
    }
}

fn set_stats(row_model: &RowModel, usage_stats: &ProcessUsageStats) {
    row_model.set_cpu_usage(usage_stats.cpu_usage);
    row_model.set_memory_usage(usage_stats.memory_usage);
    row_model.set_shared_memory_usage(usage_stats.shared_memory_usage);
    row_model.set_disk_usage(usage_stats.disk_usage);
    row_model.set_network_usage(usage_stats.network_usage);
    row_model.set_gpu_usage(usage_stats.gpu_usage);
    row_model.set_gpu_memory_usage(usage_stats.gpu_memory_usage);
}

fn set_service(row_model: &RowModel, service: &Service) {
    row_model.set_service_running(service.running);
    row_model.set_service_enabled(service.enabled);
    row_model.set_service_failed(service.failed);
    row_model.set_service_stopped(!service.running && !service.failed && service.enabled);
}

fn primary_processes<'a>(app: &App, process_map: &'a HashMap<u32, Process>) -> Vec<&'a Process> {
    let mut secondary_processes = HashSet::new();
    for app_pid in app.pids.iter() {
        if let Some(process) = process_map.get(app_pid) {
            for child in &process.children {
                if app.pids.contains(child) {
                    secondary_processes.insert(*child);
                }
            }
        }
    }

    let mut primary_processes = Vec::new();
    for app_pid in app.pids.iter() {
        if let Some(process) = process_map.get(app_pid) {
            if !secondary_processes.contains(&process.pid) {
                primary_processes.push(process);
            }
        }
    }

    if primary_processes.is_empty() {
        for (index, pid) in app.pids.iter().enumerate() {
            if let Some(process) = process_map.get(pid) {
                if process.children.len() > 0 || index == app.pids.len() - 1 {
                    primary_processes.push(process);
                    break;
                }
            }
        }
    }

    primary_processes
}

pub fn update_apps(
    app_map: &HashMap<String, App>,
    process_map: &HashMap<u32, Process>,
    process_model_map: &HashMap<u32, RowModel>,
    app_icons: &mut HashMap<u32, String>,
    list: &gio::ListStore,
) {
    app_icons.clear();

    let mut to_remove = Vec::with_capacity(list.n_items() as _);
    for i in (0..list.n_items()).rev() {
        let Some(app) = list.item(i).and_then(|obj| obj.downcast::<RowModel>().ok()) else {
            to_remove.push(i);
            continue;
        };

        if app_map.contains_key(app.id().as_str()) {
            continue;
        }

        to_remove.push(i);
    }
    for i in &to_remove {
        list.remove(*i);
    }

    for app in app_map.values() {
        let primary_processes = primary_processes(app, process_map);
        if primary_processes.is_empty() {
            g_critical!(
                "MissionCenter::AppsPage",
                "Failed to find primary PID for app {}",
                app.name
            );
            continue;
        }

        let row_model = if let Some(index) = list.find_with_equal_func(|obj| {
            let Some(row_model) = obj.downcast_ref::<RowModel>() else {
                return false;
            };
            row_model.id() == app.id
        }) {
            unsafe {
                list.item(index)
                    .and_then(|obj| obj.downcast().ok())
                    .unwrap_unchecked()
            }
        } else {
            let row_model = RowModelBuilder::new()
                .content_type(ContentType::App)
                .section_type(SectionType::FirstSection)
                .id(&app.id)
                .build();
            list.append(&row_model);
            row_model
        };

        let icon = app
            .icon
            .as_ref()
            .map(|i| match &i.icon {
                Some(Icon::Path(p)) => p,
                Some(Icon::Id(i)) => i,
                _ => "application-x-executable",
            })
            .unwrap_or("application-x-executable");

        to_remove.clear();
        let app_children = row_model.children();
        for i in (0..app_children.n_items()).rev() {
            let Some(child) = app_children
                .item(i)
                .and_then(|obj| obj.downcast::<RowModel>().ok())
            else {
                to_remove.push(i);
                continue;
            };

            if primary_processes.iter().any(|p| p.pid == child.pid()) {
                continue;
            }

            to_remove.push(i);
        }
        for i in &to_remove {
            app_children.remove(*i);
        }

        let mut usage_stats = ProcessUsageStats::default();
        for process in &primary_processes {
            usage_stats.merge(&process.merged_usage_stats(&process_map));
            app_icons.insert(process.pid, icon.to_string());

            if app_children
                .find_with_equal_func(|p| {
                    if let Some(obj) = p.downcast_ref::<RowModel>() {
                        obj.pid() == process.pid
                    } else {
                        false
                    }
                })
                .is_some()
            {
                continue;
            }

            if let Some(row_model) = process_model_map.get(&process.pid) {
                app_children.append(row_model);
            }
        }

        row_model.set_name(app.name.as_str());
        row_model.set_icon(icon);

        set_stats(&row_model, &usage_stats);
    }
}

pub fn update_processes(
    process_map: &HashMap<u32, Process>,
    pid: &u32,
    list: &gio::ListStore,
    app_icons: &HashMap<u32, String>,
    icon: &str,
    use_merged_stats: bool,
    models: &mut HashMap<u32, RowModel>,
) {
    let Some(process) = process_map.get(&pid) else {
        return;
    };

    let pretty_name = if process.exe.is_empty() {
        if let Some(cmd) = process.cmd.first() {
            let mut cmd = cmd
                .split_ascii_whitespace()
                .next()
                .and_then(|s| s.split('/').last())
                .unwrap_or(&process.name);
            if let Some(s) = cmd.strip_suffix(':') {
                cmd = s;
            }
            cmd.trim()
        } else {
            process.name.trim()
        }
    } else {
        let exe_name = process.exe.split('/').last().unwrap_or(&process.name);
        if exe_name.starts_with("wine") {
            if process.cmd.is_empty() {
                process.name.trim()
            } else {
                process.cmd[0]
                    .split("\\")
                    .last()
                    .unwrap_or(&process.name)
                    .split("/")
                    .last()
                    .unwrap_or(&process.name)
                    .trim()
            }
        } else {
            exe_name.trim()
        }
    };

    let row_model = if let Some(index) = list.find_with_equal_func(|obj| {
        let Some(row_model) = obj.downcast_ref::<RowModel>() else {
            return false;
        };
        row_model.pid() == process.pid
    }) {
        unsafe {
            list.item(index)
                .and_then(|obj| obj.downcast().ok())
                .unwrap_unchecked()
        }
    } else {
        let row_model = RowModelBuilder::new()
            .content_type(ContentType::Process)
            .section_type(SectionType::SecondSection)
            .id(&process.pid.to_string())
            .build();
        list.append(&row_model);
        row_model
    };

    let prev_children = row_model.children();
    let mut to_remove = Vec::with_capacity(prev_children.n_items() as _);
    for i in (0..prev_children.n_items()).rev() {
        let Some(child) = prev_children
            .item(i)
            .and_then(|obj| obj.downcast::<RowModel>().ok())
        else {
            to_remove.push(i);
            continue;
        };

        if process.children.contains(&child.pid()) {
            continue;
        }

        to_remove.push(i);
    }
    for i in to_remove {
        prev_children.remove(i);
    }

    let merged_usage_stats = process.merged_usage_stats(&process_map);
    let usage_stats = if use_merged_stats {
        &merged_usage_stats
    } else {
        &process.usage_stats
    };

    let icon = if let Some(icon) = app_icons.get(&process.pid) {
        icon.as_str()
    } else {
        icon
    };

    let command_line = process.cmd.join(" ");

    row_model.set_name(pretty_name);
    row_model.set_icon(icon);
    row_model.set_command_line(command_line);
    row_model.set_pid(process.pid);

    set_stats(&row_model, usage_stats);

    for child in &process.children {
        update_processes(
            process_map,
            child,
            &row_model.children(),
            app_icons,
            icon,
            use_merged_stats,
            models,
        );
    }

    models.insert(process.pid, row_model);
}
