use std::collections::HashMap;
use std::hash::Hash;
use adw::{gdk, gio};
use adw::glib::Bytes;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::Image;
use magpie_types::apps::icon::Icon;
use crate::app;

#[derive(Default, Clone)]
pub enum CachedIcon {
    #[default]
    Empty,
    Id(String),
    CachedData((Bytes, HashMap<i32, Option<Pixbuf>>)),
}

/// A light-weight version of CachedIcon that tells you where to find the real icon rather than
/// passing around a heavy raw img
#[derive(Clone, Default, Eq, PartialEq)]
pub enum LightCachedIcon {
    #[default]
    Empty,
    /// This contains the actual img, use `set_icon_from_stringlike`
    StringPayload(String),
    /// This value is the key in the apps_cache in the apps obj
    AppCachedKey(String, i32),
}

impl From<Icon> for CachedIcon {
    fn from(icon: Icon) -> Self {
        match icon {
            Icon::Empty(_) => { Self::Empty }
            Icon::Id(id) => { Self::Id(id) }
            Icon::Data(v8) => { Self::CachedData((Bytes::from(&v8), Default::default()))}
        }
    }
}

impl From<Vec<u8>> for CachedIcon {
    fn from(icon: Vec<u8>) -> Self {
        CachedIcon::CachedData((Bytes::from(&icon), Default::default()))
    }
}

impl CachedIcon {
    #[inline]
    pub fn apply_blank(image: &Image) -> bool {
        image.set_icon_name(Some("application-x-executable"));

        false
    }

    pub fn set_icon_from_stringlike(image: &Image, icon_name: &str) -> bool {
        let display = gdk::Display::default().unwrap();
        let icon_theme = gtk::IconTheme::for_display(&display);

        if icon_theme.has_icon(icon_name) {
            image.set_icon_name(Some(icon_name));

            true
        } else {
            Self::apply_blank(image);

            false
        }
    }

    pub fn apply_to_image(&mut self, image: &Image, width: i32) -> bool {
        #[allow(deprecated)]
        fn apply_buf(image: &Image, buf: Option<&Pixbuf>) -> bool {
            image.set_from_pixbuf(buf);

            true
        }

        fn create_img_cache(bytes: &Bytes, width: i32) -> Option<Pixbuf> {
            let input_stream = gio::MemoryInputStream::from_bytes(bytes);

            Pixbuf::from_stream_at_scale(
                &input_stream,
                width,
                -1,
                true,
                None::<&gio::Cancellable>,
            ).ok()
        }

        match self {
            Self::Empty => Self::apply_blank(image),
            Self::Id(id) => Self::set_icon_from_stringlike(image, &id),
            Self::CachedData((img, caches)) => {
                let pixbuf = match caches.get(&width) {
                    Some(pixbuf) => pixbuf,
                    None => {
                        let out = create_img_cache(img, width);

                        caches.insert(width, out);

                        caches.get(&width).unwrap()
                    }
                };

                if pixbuf.is_some() {
                    apply_buf(image, pixbuf.as_ref());

                    true
                } else {
                    Self::apply_blank(image);

                    false
                }
            },
        }
    }

    pub fn convert_hash_map<K>(mut map: HashMap<K, Icon>) -> HashMap<K, CachedIcon> where K: Hash + Eq + Clone {
        map.drain()
            .map(|(k, v)| (k, CachedIcon::from(v)))
            .collect()
    }

    pub fn get_light_icon(&self, payload: Option<(String, i32)>) -> LightCachedIcon {
        match self {
            CachedIcon::Empty => { LightCachedIcon::Empty }
            CachedIcon::Id(s) => { LightCachedIcon::StringPayload(s.clone()) }
            // todo warn?
            CachedIcon::CachedData(_) => { payload.map(|(key, size)| LightCachedIcon::AppCachedKey(key,size)).unwrap_or_default() }
        }
    }
}

impl LightCachedIcon {
    pub fn apply_to_image(&self, image: &Image) -> bool {
        match self {
            LightCachedIcon::Empty => { CachedIcon::apply_blank(image) }
            LightCachedIcon::StringPayload(p) => { CachedIcon::set_icon_from_stringlike(image, p) }
            LightCachedIcon::AppCachedKey(k, v) => { app!().apply_app_icon(image, k.clone(), v.clone()) }
        }
    }

    pub fn apply_to_image_custom_size(&self, image: &Image, w: i32) -> bool {
        match self {
            LightCachedIcon::Empty => { CachedIcon::apply_blank(image) }
            LightCachedIcon::StringPayload(p) => { CachedIcon::set_icon_from_stringlike(image, p) }
            LightCachedIcon::AppCachedKey(k, _) => { app!().apply_app_icon(image, k.clone(), w) }
        }
    }
}
