/// Translate a message ID with optional arguments.
///
/// Returns a reactive [`Signal<String>`] that re-translates on locale
/// change.  Requires `I18n` via [`MountContext::i18n`](crate::core::context::MountContext::i18n).
///
/// # Usage
///
/// ```ignore
/// let greeting = t!(ctx, "hello", name = user_name);
/// Box::new(Text::new(greeting.read()).bind(greeting)).mount_static(ctx)
/// ```
///
/// Each argument value is converted via [`IntoFluentValue`].
#[macro_export]
macro_rules! t {
    ($ctx:expr, $msg_id:literal $(, $key:ident = $val:expr)*) => {{
        #[cfg(feature = "i18n")]
        {
            let __i18n_ctx = $ctx.i18n.expect(
                "t! requires I18n on MountContext (enable i18n feature and provide I18n via WindowConfig)"
            );
            let __args: &[(&str, $crate::i18n::bundle::FluentValue)] = &[
                $((stringify!($key),
                   $crate::i18n::bundle::IntoFluentValue::to_fluent_value(&$val))),*
            ];
            $crate::i18n::tr_signal(__i18n_ctx, $msg_id, __args)
        }
        #[cfg(not(feature = "i18n"))]
        {
            let _ = ($($key = &$val),*);
            let _ = $ctx;
            $crate::auralis_signal::Signal::new($msg_id.to_string())
        }
    }};
}
