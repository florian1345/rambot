use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    ModifierDocumentation,
    ModifierDocumentationBuilder,
    Plugin,
    PluginConfig,
    ResolverRegistry
};

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

use std::collections::{HashMap, HashSet};
use std::io;

fn shuffle<T, R: Rng>(slice: &mut [T], rng: &mut R) {
    for i in (1..slice.len()).rev() {
        let j = rng.gen_range(0..=i);
        slice.swap(i, j);
    }
}

struct ShuffleAudioSourceList<R> {
    child: Box<dyn AudioSourceList + Send>,
    next: Option<String>,
    buf: Vec<String>,
    rng: R
}

impl<R: Rng> AudioSourceList for ShuffleAudioSourceList<R> {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        if self.buf.is_empty() {
            let mut distinct = HashSet::new();

            if let Some(next) = self.next.take() {
                distinct.insert(next);
            }

            while let Some(s) = self.child.next()? {
                if distinct.contains(&s) {
                    self.next = Some(s);
                    break;
                }

                distinct.insert(s);
            }

            self.buf.extend(distinct.into_iter());
            shuffle(&mut self.buf, &mut self.rng);
        }

        Ok(self.buf.pop())
    }
}

struct ShuffleAdapterResolver;

impl AdapterResolver for ShuffleAdapterResolver {
    fn name(&self) -> &str {
        "shuffle"
    }

    fn unique(&self) -> bool {
        true
    }

    fn documentation(&self) -> ModifierDocumentation {
        ModifierDocumentationBuilder::new()
            .with_short_summary("Shuffles a playlist randomly.")
            .with_long_summary("Shuffles a playlist randomly. In case of an \
                infinite list (e.g. a looped list), the first segment of \
                distinct entries until the first duplicate is shuffled, \
                followed by the second segment etc.")
            .build().unwrap()
    }

    fn resolve(&self, _key_values: &HashMap<String, String>,
            child: Box<dyn AudioSourceList + Send>)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        Ok(Box::new(ShuffleAudioSourceList {
            child,
            next: None,
            buf: Vec::new(),
            rng: SmallRng::from_rng(rand::thread_rng()).unwrap()
        }))
    }
}

struct ShufflePlugin;

impl Plugin for ShufflePlugin {
    fn load_plugin<'registry>(&self, _config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_adapter_resolver(ShuffleAdapterResolver);
        Ok(())
    }
}

fn make_shuffle_plugin() -> ShufflePlugin {
    ShufflePlugin
}

rambot_api::export_plugin!(make_shuffle_plugin);

