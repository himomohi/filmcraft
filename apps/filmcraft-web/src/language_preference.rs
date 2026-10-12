//! Remember the interface language on this browser origin, including an explicit System Language.
//! Storage may be disabled or full: the app keeps working and reports a failed save once per change.

use std::cell::RefCell;

use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use serde_json::json;

const KEY: &str = "filmcraft.interfaceLanguage";

thread_local! {
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or_else(|| "no browser window".to_string())?
        .local_storage()
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| "browser storage is unavailable".to_string())
}

pub(super) fn load(session: &mut Session) {
    match storage().and_then(|s| s.get_item(KEY).map_err(|e| format!("{e:?}"))) {
        Ok(Some(value)) => {
            // Validate the saved value through the same schema as Settings and prefs.set.
            if let Err(e) = session.execute("prefs.set", json!({"key": "general.interfaceLanguage", "value": value})) {
                log::warn!("ignoring invalid saved interface language: {e}");
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("loading interface language: {e}"),
    }
    LAST.with(|last| *last.borrow_mut() = Some(session.prefs.general.interface_language.clone()));
}

pub(super) fn save(app: &mut FilmcraftApp) {
    let value = app.session.prefs.general.interface_language.clone();
    let changed = LAST.with(|last| {
        let mut last = last.borrow_mut();
        if last.as_ref() == Some(&value) {
            return false;
        }
        *last = Some(value.clone());
        true
    });
    if changed && let Err(e) = storage().and_then(|s| s.set_item(KEY, &value).map_err(|e| format!("{e:?}"))) {
        log::warn!("saving interface language: {e}");
        app.status(filmcraft_ui_egui::i18n::fmt(filmcraft_ui_egui::i18n::t("The language changed but could not be saved: {e}"), &[("e", &e)]));
    }
}
