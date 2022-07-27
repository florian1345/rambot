mod kernel;
mod util;

use crate::kernel::KernelFilter;

use rambot_api::{
    AudioSource,
    EffectResolver,
    ModifierDocumentation,
    ModifierDocumentationBuilder,
    Plugin,
    PluginConfig,
    ResolveEffectError,
    ResolverRegistry
};

use std::collections::HashMap;

fn get_sigma(key_values: &HashMap<String, String>) -> Result<f32, String> {
    key_values.get("sigma")
        .ok_or_else(|| "Missing argument \"sigma\".".to_owned())
        .and_then(|s|
            s.parse().map_err(
                |e| format!("Error parsing value for \"sigma\": {}.", e)))
}

struct GaussianEffectResolver;

impl EffectResolver for GaussianEffectResolver {

    fn name(&self) -> &str {
        "gaussian"
    }

    fn unique(&self) -> bool {
        false
    }

    fn documentation(&self) -> ModifierDocumentation {
        ModifierDocumentationBuilder::new()
            .with_short_summary("Applies a gaussian lowpass filter to the \
                audio.")
            .with_parameter("sigma", "The width of the gaussian kernel. \
                Higher values cause lower frequencies to be cut. \
                Experimentation is required. Typical values are in the range \
                1 to 100.")
            .build().unwrap()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveEffectError> {
        let sigma = match get_sigma(key_values) {
            Ok(s) => s,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        Ok(Box::new(KernelFilter::new(child, kernel::gaussian(sigma))))
    }
}

struct InvGaussianEffectResolver;

impl EffectResolver for InvGaussianEffectResolver {

    fn name(&self) -> &str {
        "inv_gaussian"
    }

    fn unique(&self) -> bool {
        false
    }

    fn documentation(&self) -> ModifierDocumentation {
        ModifierDocumentationBuilder::new()
            .with_short_summary("Subtracts a gaussian lowpass filter from the \
                audio, thus obtaining a highpass filter.")
            .with_parameter("sigma", "The width of the gaussian kernel. \
                Higher values cause less higher frequencies to be cut. \
                Experimentation is required. Typical values are in the range \
                1 to 100.")
            .build().unwrap()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveEffectError> {
        let sigma = match get_sigma(key_values) {
            Ok(s) => s,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        Ok(Box::new(KernelFilter::new(child, kernel::inv_gaussian(sigma))))
    }
}

struct FiltersPlugin;

impl Plugin for FiltersPlugin {

    fn load_plugin<'registry>(&mut self, _config: &PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_effect_resolver(GaussianEffectResolver);
        registry.register_effect_resolver(InvGaussianEffectResolver);

        Ok(())
    }
}

fn make_filters_plugin() -> FiltersPlugin {
    FiltersPlugin
}

rambot_api::export_plugin!(make_filters_plugin);
