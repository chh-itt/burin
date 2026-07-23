//! Internationalization via Project Fluent.
//!
//! Enabled by the `i18n` feature.  When disabled, the `t!` macro returns
//! a static signal with the message ID as the value.
//!
//! # Quick start
//!
//! ```ignore
//! use burin::i18n::{I18n, I18nBuilder};
//!
//! let i18n = I18n::builder()
//!     .add_resource("en-US".parse().unwrap(), include_str!("locales/en-US/main.ftl"))
//!     .add_resource("zh-CN".parse().unwrap(), include_str!("locales/zh-CN/main.ftl"))
//!     .build()
//!     .expect("i18n init");
//!
//! App::new()
//!     .window(WindowConfig { i18n: Some(i18n), ..Default::default() }, my_widget)
//!     .run();
//! ```
//!
//! In a widget:
//!
//! ```ignore
//! fn mount_static(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
//!     let label = t!(ctx, "hello", name = user_name);
//!     Box::new(Text::new(label.read()).bind(label)).mount_static(ctx)
//! }
//! ```

pub mod bundle;
mod locale;
mod macros;
mod provider;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use auralis_signal::Signal;
pub use locale::system_locale;
use unic_langid::LanguageIdentifier;

pub use bundle::{FluentArgs, FluentValue, IntoFluentValue};
pub use provider::tr_signal;

/// Internationalization (i18n) instance backed by Project Fluent.
///
/// Clone is cheap (reference-counted).
#[derive(Clone)]
pub struct I18n {
    pub(crate) inner: Rc<I18nInner>,
}

pub(crate) struct I18nInner {
    pub(crate) bundles:
        RefCell<HashMap<LanguageIdentifier, fluent::FluentBundle<fluent::FluentResource>>>,
    pub(crate) current_locale: RefCell<LanguageIdentifier>,
    pub(crate) fallback_locale: LanguageIdentifier,
    pub(crate) locale_changed: Signal<(LanguageIdentifier, LanguageIdentifier)>,
}

impl I18n {
    /// Create a new [`I18nBuilder`].
    pub fn builder() -> I18nBuilder {
        I18nBuilder::new()
    }

    /// Translate a message ID.
    ///
    /// Returns the translated string for the current locale, falling back
    /// to the fallback locale, then returning `msg_id` as-is.
    pub fn tr(&self, msg_id: &str, args: &[(&str, FluentValue)]) -> String {
        provider::tr_inner(&self.inner, msg_id, args)
    }

    /// Current active locale.
    pub fn current_locale(&self) -> LanguageIdentifier {
        self.inner.current_locale.borrow().clone()
    }

    /// Signal that fires when the locale changes: `(old, new)`.
    pub fn on_locale_changed(&self) -> &Signal<(LanguageIdentifier, LanguageIdentifier)> {
        &self.inner.locale_changed
    }

    /// Switch to a different locale.  All `tr_signal`-derived signals
    /// update automatically.
    pub fn set_locale(&self, locale: LanguageIdentifier) {
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        if !self.inner.bundles.borrow().contains_key(&locale) {
            tracing::warn!(
                locale = %locale,
                "no bundle registered for locale; translations will fall back"
            );
        }
        let old = self.inner.current_locale.replace(locale.clone());
        self.inner.locale_changed.set((old, locale));
    }

    /// Add an FTL resource for the given locale at runtime.
    pub fn add_resource(&self, locale: &LanguageIdentifier, ftl: &str) -> Result<(), I18nError> {
        let mut bundles = self.inner.bundles.borrow_mut();
        let bundle = bundles
            .entry(locale.clone())
            .or_insert_with(|| bundle::create_bundle(std::slice::from_ref(locale)));
        bundle::add_resource(bundle, ftl)
    }

    /// Check if the current locale is right-to-left.
    pub fn is_rtl(&self) -> bool {
        locale::is_rtl(&self.current_locale())
    }
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n")
            .field("current_locale", &self.current_locale().to_string())
            .field("fallback_locale", &self.inner.fallback_locale.to_string())
            .finish_non_exhaustive()
    }
}

// ── I18nBuilder ─────────────────────────────────────────────────────

/// Builder for [`I18n`].
pub struct I18nBuilder {
    resources: Vec<(LanguageIdentifier, String)>,
    initial_locale: Option<LanguageIdentifier>,
    fallback_locale: LanguageIdentifier,
}

impl I18nBuilder {
    pub fn new() -> Self {
        Self {
            resources: Vec::new(),
            initial_locale: None,
            fallback_locale: "en-US".parse().unwrap(),
        }
    }

    /// Set the initial locale.  Defaults to the system locale.
    pub fn initial_locale(mut self, locale: LanguageIdentifier) -> Self {
        self.initial_locale = Some(locale);
        self
    }

    /// Set the fallback locale (default: `en-US`).
    pub fn fallback(mut self, locale: LanguageIdentifier) -> Self {
        self.fallback_locale = locale;
        self
    }

    /// Add an FTL resource string for a locale.
    pub fn add_resource(mut self, locale: LanguageIdentifier, ftl: impl Into<String>) -> Self {
        self.resources.push((locale, ftl.into()));
        self
    }

    /// Load all `.ftl` files from a directory.  The file stem (without
    /// extension) is used as the locale tag.
    ///
    /// This scans the directory at build time (runtime loading).
    pub fn load_dir(mut self, path: impl AsRef<std::path::Path>) -> Result<Self, I18nError> {
        for entry in std::fs::read_dir(path.as_ref()).map_err(I18nError::Io)? {
            let entry = entry.map_err(I18nError::Io)?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "ftl") {
                let ftl = std::fs::read_to_string(&path).map_err(I18nError::Io)?;
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("en-US");
                let locale: LanguageIdentifier = stem
                    .parse()
                    .map_err(|e| I18nError::Parse(format!("invalid locale tag `{stem}`: {e}")))?;
                self.resources.push((locale, ftl));
            }
        }
        Ok(self)
    }

    /// Build the [`I18n`] instance, parsing all FTL resources eagerly.
    pub fn build(self) -> Result<Rc<I18n>, I18nError> {
        let initial_locale = self.initial_locale.unwrap_or_else(locale::system_locale);

        let mut bundles: HashMap<LanguageIdentifier, fluent::FluentBundle<fluent::FluentResource>> =
            HashMap::new();

        for (locale, ftl) in &self.resources {
            let bundle = bundles
                .entry(locale.clone())
                .or_insert_with(|| bundle::create_bundle(std::slice::from_ref(locale)));
            bundle::add_resource(bundle, ftl)?;
        }

        let inner = I18nInner {
            bundles: RefCell::new(bundles),
            current_locale: RefCell::new(initial_locale.clone()),
            fallback_locale: self.fallback_locale,
            locale_changed: Signal::new((initial_locale.clone(), initial_locale)),
        };

        Ok(Rc::new(I18n {
            inner: Rc::new(inner),
        }))
    }
}

impl Default for I18nBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── I18nError ───────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum I18nError {
    #[error("FTL parse error: {0}")]
    Parse(String),
    #[error("failed to add resource: {0}")]
    AddResource(String),
    #[error("I/O error: {0}")]
    #[cfg_attr(feature = "devtools", serde(skip))]
    Io(std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_empty() {
        let i18n = I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .build()
            .unwrap();
        assert_eq!(i18n.current_locale().to_string(), "en-US");
    }

    #[test]
    fn translate_simple() {
        let i18n = I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .add_resource("en-US".parse().unwrap(), "hello = Hello, World!")
            .build()
            .unwrap();
        assert_eq!(i18n.tr("hello", &[]), "Hello, World!");
    }

    #[test]
    fn translate_missing_key_returns_key() {
        let i18n = I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .build()
            .unwrap();
        assert_eq!(i18n.tr("nonexistent", &[]), "nonexistent");
    }

    #[test]
    fn translate_with_args() {
        let i18n = I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .add_resource("en-US".parse().unwrap(), "greeting = Hello, { $name }!")
            .build()
            .unwrap();
        assert_eq!(
            i18n.tr("greeting", &[("name", FluentValue::from("World"))]),
            "Hello, World!"
        );
    }

    #[test]
    fn fallback_locale() {
        let i18n = I18n::builder()
            .initial_locale("zh-CN".parse().unwrap())
            .fallback("en-US".parse().unwrap())
            .add_resource("en-US".parse().unwrap(), "hello = Hello!")
            .build()
            .unwrap();
        // No zh-CN resource, falls back to en-US
        assert_eq!(i18n.tr("hello", &[]), "Hello!");
    }

    #[test]
    fn set_locale_triggers_signal() {
        use std::cell::Cell;
        let i18n = I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .build()
            .unwrap();
        let observed = std::rc::Rc::new(Cell::new(false));
        let observed_clone = observed.clone();
        let _sub =
            auralis_signal::subscription::subscribe_to(i18n.on_locale_changed(), move || {
                observed_clone.set(true)
            });
        i18n.set_locale("zh-CN".parse().unwrap());
        assert!(observed.get());
    }

    #[test]
    fn rtl_detection() {
        let i18n = I18n::builder()
            .initial_locale("ar".parse().unwrap())
            .build()
            .unwrap();
        assert!(i18n.is_rtl());
    }

    // ── tr_signal lifetime (self-cleaning derived subscriptions) ──────

    fn build_bilingual() -> Rc<I18n> {
        I18n::builder()
            .initial_locale("en-US".parse().unwrap())
            .add_resource("en-US".parse().unwrap(), "hello = Hello!")
            .add_resource("zh-CN".parse().unwrap(), "hello = 你好！")
            .build()
            .unwrap()
    }

    #[test]
    fn tr_signal_updates_on_locale_change() {
        auralis_signal::install_schedule_hook(Box::new(|f| f()));
        let i18n = build_bilingual();
        let label = tr_signal(&i18n, "hello", &[]);
        assert_eq!(label.read(), "Hello!");
        i18n.set_locale("zh-CN".parse().unwrap());
        assert_eq!(label.read(), "你好！");
    }

    #[test]
    fn tr_signal_unsubscribes_after_derived_signal_dropped() {
        auralis_signal::install_schedule_hook(Box::new(|f| f()));
        let i18n = build_bilingual();
        let label = tr_signal(&i18n, "hello", &[]);
        assert_eq!(i18n.on_locale_changed().subscriber_count(), 1);

        drop(label);
        // First locale change after the consumer died: the derived
        // callback sees a dead weak target and removes itself.
        i18n.set_locale("zh-CN".parse().unwrap());
        assert_eq!(
            i18n.on_locale_changed().subscriber_count(),
            0,
            "dropped tr_signal must not leave a live locale subscription"
        );
        // And stays clean on further churn.
        i18n.set_locale("en-US".parse().unwrap());
        assert_eq!(i18n.on_locale_changed().subscriber_count(), 0);
    }

    #[test]
    fn tr_signal_clone_keeps_updates_flowing() {
        auralis_signal::install_schedule_hook(Box::new(|f| f()));
        let i18n = build_bilingual();
        let label = tr_signal(&i18n, "hello", &[]);
        let shared = label.clone();

        drop(label);
        // A live clone must keep receiving translations.
        i18n.set_locale("zh-CN".parse().unwrap());
        assert_eq!(shared.read(), "你好！");
        assert_eq!(i18n.on_locale_changed().subscriber_count(), 1);
    }

    #[test]
    fn tr_signal_churn_does_not_accumulate_subscriptions() {
        auralis_signal::install_schedule_hook(Box::new(|f| f()));
        let i18n = build_bilingual();
        // Simulates dynamic widget rebuild cycles (list items, tab
        // switches): each cycle creates and drops a tr_signal.
        for _ in 0..50 {
            let label = tr_signal(&i18n, "hello", &[]);
            let _ = label.read();
        }
        // One locale flip garbage-collects every dead subscription.
        i18n.set_locale("zh-CN".parse().unwrap());
        assert_eq!(i18n.on_locale_changed().subscriber_count(), 0);
    }

    #[test]
    fn tr_signal_does_not_keep_i18n_alive() {
        auralis_signal::install_schedule_hook(Box::new(|f| f()));
        let i18n = build_bilingual();
        let weak_inner = Rc::downgrade(&i18n.inner);
        let label = tr_signal(&i18n, "hello", &[]);
        drop(i18n);
        // The derived subscription must not pin the I18n allocation:
        // callback captures Weak<I18nInner>, and the subscription lives
        // inside locale_changed which dies with I18nInner itself.
        assert!(
            weak_inner.upgrade().is_none(),
            "tr_signal must not create an Rc cycle that pins I18nInner"
        );
        // The derived signal keeps its last value; reading is safe.
        assert_eq!(label.read(), "Hello!");
    }
}
