mod config;
mod echo;
mod kernel;
mod util;

use crate::config::Config;
use crate::echo::EchoEffect;
use crate::kernel::KernelFilter;

use rambot_api::{
    AudioSource,
    EffectResolver,
    ModifierDocumentation,
    ModifierDocumentationBuilder,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolveEffectError,
    ResolverRegistry
};

use std::{collections::HashMap, str::FromStr, fmt::Display};

fn get_mandatory<T>(key_values: &HashMap<String, String>, key: &str)
    -> Result<T, String>
where
    T: FromStr,
    T::Err: Display
{
    key_values.get(key)
        .ok_or_else(|| format!("Missing argument \"{}\".", key))
        .and_then(|s|
            s.parse().map_err(
                |e| format!("Error parsing value for \"{}\": {}.", key, e)))
}

fn get_sigma(key_values: &HashMap<String, String>) -> Result<f32, String> {
    get_mandatory(key_values, "sigma")
}

fn get_kernel_size_sigmas(key_values: &HashMap<String, String>, config: &Config) -> Result<f32, String> {
    key_values.get("kernel_size")
        .map(|s| s.parse())
        .unwrap_or_else(|| Ok(config.default_gaussian_kernel_size_sigmas()))
        .map_err(|e|
            format!("Error parsing value for \"kernel_size\": {}.", e))
}

fn resolve_gaussian_like_kernel_filter<F>(key_values: &HashMap<String, String>,
    child: Box<dyn AudioSource + Send + Sync>, config: &Config, gen_kernel: F)
    -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError>
where
    F: Fn(f32, f32) -> Vec<f32>
{
    let sigma = match get_sigma(key_values) {
        Ok(s) => s,
        Err(msg) => return Err(ResolveEffectError::new(msg, child))
    };
    let kernel_size_sigmas_res = get_kernel_size_sigmas(key_values, config);
    let kernel_size_sigmas = match kernel_size_sigmas_res {
        Ok(ks) => ks,
        Err(msg) => return Err(ResolveEffectError::new(msg, child))
    };
    let kernel = gen_kernel(sigma, kernel_size_sigmas);
    let max_size = config.max_kernel_size_samples();

    if max_size == 0 || kernel.len() <= max_size {
        Ok(Box::new(KernelFilter::new(child, kernel)))
    }
    else {
        let msg = format!("Kernel has total size {}, but the maximum is {}. \
            Reduce `sigma` or `kernel_size`.", kernel.len(), max_size);

        Err(ResolveEffectError::new(msg, child))
    }
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
            child: Box<dyn AudioSource + Send + Sync>,
            _guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError> {
        resolve_gaussian_like_kernel_filter(
            key_values, child, &self.config, kernel::gaussian)
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
            child: Box<dyn AudioSource + Send + Sync>,
            _plugin_guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError> {
        resolve_gaussian_like_kernel_filter(
            key_values, child, &self.config, kernel::inv_gaussian)
    }
}

struct EchoEffectResolver {
    config: Config
}

impl EffectResolver for EchoEffectResolver {

    fn name(&self) -> &str {
        "echo"
    }

    fn unique(&self) -> bool {
        false
    }

    fn documentation(&self) -> ModifierDocumentation {
        ModifierDocumentationBuilder::new()
            .with_short_summary("Adds a delayed and scaled copy of the audio \
                to itself, resulting in an echo effect.")
            .with_parameter("delay", "The delay of the first echo. Input of \
                the format `AhBmCsDmsEsam`, representing `A` hours, `B` \
                minutes, `C` seconds, `D` milliseconds, and `E` samples (at \
                48 kHz). Omitting and reordering these terms is permitted.")
            .with_parameter("factor", "The volume applied to each iteration of
                the echo. Must be less than 1 in order to avoid catastrophe.")
            .build().unwrap()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send + Sync>,
            _guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError> {
        let delay = match get_mandatory(key_values, "delay") {
            Ok(d) => d,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        if delay > self.config.max_echo_delay() {
            return Err(ResolveEffectError::new(
                format!("Delay may be at most `{}`.",
                    self.config.max_echo_delay()), child));
        }

        let factor = match get_mandatory(key_values, "factor") {
            Ok(f) => f,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };
        let effect = EchoEffect::new(child, delay, factor).map_err(|child|
            ResolveEffectError::new(
                "Invalid delay. Must be positive and not too large."
                .to_owned(), child))?;

        Ok(Box::new(effect))
    }
}

struct FiltersPlugin;

impl Plugin for FiltersPlugin {

    fn load_plugin(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'_>) -> Result<(), String> {
        let config = Config::load(config.config_path())?;

        registry.register_effect_resolver(GaussianEffectResolver {
            config: config.clone()
        });

        registry.register_effect_resolver(InvGaussianEffectResolver {
            config: config.clone()
        });

        registry.register_effect_resolver(EchoEffectResolver {
            config
        });

        Ok(())
    }
}

fn make_filters_plugin() -> FiltersPlugin {
    FiltersPlugin
}

rambot_api::export_plugin!(make_filters_plugin);
