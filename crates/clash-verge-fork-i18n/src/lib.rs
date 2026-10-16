//! Fork-local translations merged into `clash_verge-i18n` at runtime.
//!
//! rust-i18n consults an extension backend before the host backend's own
//! data, so keys nested under `clash_verge_i18n` in `locales/` resolve for
//! every `clash_verge_i18n::t!` call once [`register`] runs. Fork strings
//! therefore never need to be added to the upstream-owned locale files.

rust_i18n::i18n!("locales", fallback = "zh");

/// Attach these translations to the host backend.
///
/// Idempotent: the host backend panics when extended twice.
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        rust_i18n::extend!(clash_verge_i18n);
    });
}
