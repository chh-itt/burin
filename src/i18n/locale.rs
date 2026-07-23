use unic_langid::LanguageIdentifier;

/// Resolve the best matching locale from a list of available locales
/// against a user's preferred locale list (with fallback).
// TODO: Integrate into the i18n fallback chain (currently FluentBundle
// handles basic negotiation; LocaleMatcher would enable sub-tag fallback
// like zh-CN → zh).
#[allow(dead_code)]
pub struct LocaleMatcher {
    available: Vec<LanguageIdentifier>,
}

#[allow(dead_code)] // reserved: sub-tag locale negotiation (see TODO above)
impl LocaleMatcher {
    pub fn new(available: Vec<LanguageIdentifier>) -> Self {
        Self { available }
    }

    /// Find the best match for `desired` against available locales.
    /// Falls back through subtag truncation (e.g. zh-Hans-CN → zh-Hans → zh).
    pub fn match_locale(&self, desired: &LanguageIdentifier) -> Option<&LanguageIdentifier> {
        // Exact match first
        if let Some(found) = self.available.iter().find(|a| *a == desired) {
            return Some(found);
        }
        // Try progressively shorter subtags (zh-Hans-CN → zh-Hans → zh)
        let desired_str = desired.to_string();
        let mut parts: Vec<&str> = desired_str.split('-').collect();
        while parts.len() > 1 {
            parts.pop();
            let candidate: LanguageIdentifier = parts.join("-").parse().ok()?;
            if let Some(found) = self.available.iter().find(|a| *a == &candidate) {
                return Some(found);
            }
        }
        None
    }
}

/// Detect the system locale.
pub fn system_locale() -> LanguageIdentifier {
    sys_locale::get_locale()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| "en-US".parse::<LanguageIdentifier>().unwrap())
}

/// Check if a locale is right-to-left.
pub fn is_rtl(locale: &LanguageIdentifier) -> bool {
    let s = locale.to_string();
    let lang = s.split('-').next().unwrap_or(&s);
    // List of RTL language codes. See: https://www.unicode.org/cldr/charts/latest/supplemental/languages_and_scripts.html
    matches!(
        lang,
        "ar" | "he" | "fa" | "ur" | "yi" | "dv" | "ps" | "ku" | "sd" | "ug" | "nqo"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let en: LanguageIdentifier = "en-US".parse().unwrap();
        let matcher = LocaleMatcher::new(vec![en.clone()]);
        assert_eq!(matcher.match_locale(&en), Some(&en));
    }

    #[test]
    fn fallback_truncation() {
        let en: LanguageIdentifier = "en".parse().unwrap();
        let matcher = LocaleMatcher::new(vec![en.clone()]);
        let desired: LanguageIdentifier = "en-US".parse().unwrap();
        assert_eq!(matcher.match_locale(&desired), Some(&en));
    }

    #[test]
    fn no_match() {
        let en: LanguageIdentifier = "en".parse().unwrap();
        let matcher = LocaleMatcher::new(vec![en]);
        let desired: LanguageIdentifier = "zh".parse().unwrap();
        assert!(matcher.match_locale(&desired).is_none());
    }

    #[test]
    fn system_locale_returns_valid() {
        let locale = system_locale();
        assert!(!locale.to_string().is_empty());
    }

    #[test]
    fn rtl_detection() {
        let ar: LanguageIdentifier = "ar".parse().unwrap();
        let en: LanguageIdentifier = "en".parse().unwrap();
        assert!(is_rtl(&ar));
        assert!(!is_rtl(&en));
    }
}
