mod kernel;
mod util;

use crate::kernel::KernelFilter;

use rambot_api::{
    AudioSource,
    EffectResolver,
    Plugin,
    PluginConfig,
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

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let sigma = get_sigma(key_values)?;
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

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let sigma = get_sigma(key_values)?;
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
