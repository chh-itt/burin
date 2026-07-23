use std::borrow::Cow;
use std::rc::Rc;

use auralis_signal::subscription::subscribe_derived;
use auralis_signal::Signal;

use crate::i18n::bundle::{self, FluentArgs, FluentValue};
use crate::i18n::{I18n, I18nInner};

fn owned_value(v: &FluentValue<'_>) -> FluentValue<'static> {
    match v.clone() {
        FluentValue::String(s) => FluentValue::String(Cow::Owned(s.into_owned())),
        FluentValue::Number(n) => FluentValue::Number(n),
        FluentValue::None => FluentValue::None,
        FluentValue::Error => FluentValue::Error,
        FluentValue::Custom(c) => FluentValue::Custom(c),
    }
}

/// Create a reactive [`Signal<String>`] that re-translates on locale change.
///
/// Lifetime is fully automatic (audit 2026-07-18): the locale
/// subscription holds only weak references — to the returned signal
/// (via [`subscribe_derived`]) and to the `I18n` internals. When the
/// last clone of the returned signal is dropped, the subscription
/// removes itself on the next locale change; dropping the `I18n` tears
/// the subscription down with it. No handle bookkeeping, no pruning,
/// no `Rc` cycles.
pub fn tr_signal(i18n: &Rc<I18n>, msg_id: &str, args: &[(&str, FluentValue)]) -> Signal<String> {
    let msg_id_owned = msg_id.to_string();
    let args_owned: Vec<(String, FluentValue<'static>)> = args
        .iter()
        .map(|(k, v)| (k.to_string(), owned_value(v)))
        .collect();

    let initial = translate(&i18n.inner, &msg_id_owned, &args_owned);
    let signal = Signal::new(initial);

    // Weak: the subscription must not pin I18nInner (it is stored inside
    // locale_changed, which I18nInner owns — a strong ref would be a cycle).
    let weak_inner = Rc::downgrade(&i18n.inner);
    subscribe_derived(&i18n.inner.locale_changed, &signal, move |sig| {
        if let Some(inner) = weak_inner.upgrade() {
            sig.set(translate(&inner, &msg_id_owned, &args_owned));
        }
    });

    signal
}

pub(crate) fn tr_inner(inner: &I18nInner, msg_id: &str, args: &[(&str, FluentValue)]) -> String {
    let mut fa = FluentArgs::new();
    for (k, v) in args {
        fa.set(*k, v.clone());
    }

    let current = inner.current_locale.borrow();
    if let Some(bundle) = inner.bundles.borrow().get(&current) {
        if let Some(result) = bundle::format_message(bundle, msg_id, Some(&fa)) {
            return result;
        }
    }

    let fallback = &inner.fallback_locale;
    if *current != *fallback {
        if let Some(bundle) = inner.bundles.borrow().get(fallback) {
            if let Some(result) = bundle::format_message(bundle, msg_id, Some(&fa)) {
                return result;
            }
        }
    }

    msg_id.to_string()
}

fn translate(inner: &I18nInner, msg_id: &str, args: &[(String, FluentValue)]) -> String {
    let borrowed: Vec<(&str, FluentValue)> =
        args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    tr_inner(inner, msg_id, &borrowed)
}
