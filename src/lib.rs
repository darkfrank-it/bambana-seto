use rust_i18n::i18n;
i18n!("locales", fallback = "en");

pub mod capture;
pub mod database;
pub mod ui;
