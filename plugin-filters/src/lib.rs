mod config;
mod kernel;
mod util;

use crate::config::Config;
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

fn get_kernel_size_sigmas(key_values: &HashMap<String, String>, config: &Config) -> Result<f32, String> {
    key_values.get("kernel_size")
        .map(|s| s.parse())
        .unwrap_or_else(|| Ok(config.default_gaussian_kernel_size_sigmas()))
        .map_err(|e|
            format!("Error parsing value for \"kernel_size\": {}.", e))
}

fn get_kernel_size_doc(config: &Config) -> String {
    format!("Optional. The size of the discrete kernel used for the \
        computation, measured in `sigma`s. Higher values result in higher \
        effect quality, but slower computation. Default is {}.",
        config.default_gaussian_kernel_size_sigmas())
}

struct GaussianEffectResolver {
    config: Config
}

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
            .with_parameter("sigma", "The width of the gaussian curve \
                described by the kernel. Higher values cause lower \
                frequencies to be cut. Experimentation is required. Typical \
                values are in the range 1 to 100.")
            .with_parameter("kernel_size", get_kernel_size_doc(&self.config))
            .build().unwrap()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveEffectError> {
        let sigma = match get_sigma(key_values) {
            Ok(s) => s,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };
        let kernel_size_sigmas_res =
            get_kernel_size_sigmas(key_values, &self.config);
        let kernel_size_sigmas = match kernel_size_sigmas_res {
            Ok(ks) => ks,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        Ok(Box::new(KernelFilter::new(
            child, kernel::gaussian(sigma, kernel_size_sigmas))))
    }
}

struct InvGaussianEffectResolver {
    config: Config
}

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
            .with_parameter("sigma", "The width of the gaussian curve \
                described by the kernel. Lower values cause higher \
                frequencies to be cut. Experimentation is required. Typical \
                values are in the range 1 to 100.")
            .with_parameter("kernel_size", get_kernel_size_doc(&self.config))
            .build().unwrap()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveEffectError> {
        let sigma = match get_sigma(key_values) {
            Ok(s) => s,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };
        let kernel_size_sigmas_res =
            get_kernel_size_sigmas(key_values, &self.config);
        let kernel_size_sigmas = match kernel_size_sigmas_res {
            Ok(ks) => ks,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        Ok(Box::new(KernelFilter::new(
            child, kernel::inv_gaussian(sigma, kernel_size_sigmas))))
    }
}

struct FiltersPlugin;

impl Plugin for FiltersPlugin {

    fn load_plugin<'registry>(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        let config = Config::load(config.config_path())?;

        registry.register_effect_resolver(GaussianEffectResolver {
            config: config.clone()
        });

        registry.register_effect_resolver(InvGaussianEffectResolver {
            config
        });

        Ok(())
    }
}

fn make_filters_plugin() -> FiltersPlugin {
    FiltersPlugin
}

rambot_api::export_plugin!(make_filters_plugin);
