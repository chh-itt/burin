use fluent::FluentBundle as FluentBundleInner;
use fluent::FluentResource;
pub use fluent::{FluentArgs, FluentValue};
use unic_langid::LanguageIdentifier;

use crate::i18n::I18nError;

pub(crate) fn create_bundle(locales: &[LanguageIdentifier]) -> FluentBundleInner<FluentResource> {
    let mut bundle = FluentBundleInner::new(locales.to_vec());
    bundle.set_use_isolating(false);
    bundle
}

pub(crate) fn add_resource(
    bundle: &mut FluentBundleInner<FluentResource>,
    ftl: &str,
) -> Result<(), I18nError> {
    let resource = FluentResource::try_new(ftl.to_string())
        .map_err(|(_, errors)| I18nError::Parse(format!("{:?}", errors)))?;
    bundle
        .add_resource(resource)
        .map_err(|errors| I18nError::AddResource(format!("{:?}", errors)))?;
    Ok(())
}

pub(crate) fn format_message(
    bundle: &FluentBundleInner<FluentResource>,
    msg_id: &str,
    args: Option<&FluentArgs>,
) -> Option<String> {
    let msg = bundle.get_message(msg_id)?;
    let pattern = msg.value()?;
    let mut errors = Vec::new();
    let result = bundle.format_pattern(pattern, args, &mut errors);
    Some(result.into_owned())
}

/// Trait for converting common Rust types to FluentValue.
pub trait IntoFluentValue {
    fn to_fluent_value(&self) -> FluentValue<'_>;
}

impl IntoFluentValue for &str {
    fn to_fluent_value(&self) -> FluentValue<'_> {
        FluentValue::from(*self)
    }
}

impl IntoFluentValue for String {
    fn to_fluent_value(&self) -> FluentValue<'_> {
        FluentValue::from(self.as_str())
    }
}

impl IntoFluentValue for i64 {
    fn to_fluent_value(&self) -> FluentValue<'_> {
        FluentValue::Number(fluent::types::FluentNumber {
            value: *self as f64,
            options: Default::default(),
        })
    }
}

impl IntoFluentValue for usize {
    fn to_fluent_value(&self) -> FluentValue<'_> {
        FluentValue::Number(fluent::types::FluentNumber {
            value: *self as f64,
            options: Default::default(),
        })
    }
}

impl IntoFluentValue for f64 {
    fn to_fluent_value(&self) -> FluentValue<'_> {
        FluentValue::Number(fluent::types::FluentNumber {
            value: *self,
            options: Default::default(),
        })
    }
}
