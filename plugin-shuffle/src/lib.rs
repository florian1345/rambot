use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    PluginConfig
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
    fn load_plugin(&mut self, _config: &PluginConfig) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        Vec::new()
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        Vec::new()
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        Vec::new()
    }

    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>> {
        vec![Box::new(ShuffleAdapterResolver)]
    }
}

fn make_shuffle_plugin() -> ShufflePlugin {
    ShufflePlugin
}

rambot_api::export_plugin!(make_shuffle_plugin);

