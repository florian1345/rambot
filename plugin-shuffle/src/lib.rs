use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    ModifierDocumentation,
    ModifierDocumentationBuilder,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
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
    child: Box<dyn AudioSourceList + Send + Sync>,
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
            child: Box<dyn AudioSourceList + Send + Sync>,
            _guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSourceList + Send + Sync>, String> {
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

#[cfg(test)]
mod tests {

    use super::*;

    use rambot_test_util::MockAudioSourceList;

    fn make_shuffle(child: MockAudioSourceList) -> Box<dyn AudioSourceList + Send + Sync> {
        let guild_config = PluginGuildConfig::default();
        let child = Box::new(child);

        ShuffleAdapterResolver.resolve(&HashMap::new(), child, guild_config)
            .unwrap()
    }

    fn collect_shuffle(entries: Vec<&str>) -> Vec<String> {
        let child = MockAudioSourceList::new(entries);
        let mut looped = make_shuffle(child);

        rambot_test_util::collect_list(&mut looped, usize::MAX).unwrap()
    }

    #[test]
    fn empty() {
        assert!(collect_shuffle(vec![]).is_empty());
    }

    #[test]
    fn singleton() {
        assert_eq!(vec!["hello".to_owned()], collect_shuffle(vec!["hello"]));
    }

    #[test]
    fn three_entries() {
        // With three entries, there are 3! = 6 configurations. We expect each
        // one to occur 18000 / 6 = 3000 times. The standard deviation is
        // sqrt(18000 * 1/6 * 5/6) = 50, so with 10 sigma certainty, the
        // number of occurrences for each configuration should fall within the
        // interval [3000 - 500, 3000 + 500] = [2500, 3500].

        const REPETITIONS: usize = 18000;
        const MIN_EXPECTED: usize = 2500;
        const MAX_EXPECTED: usize = 3500;

        let mut occurrences: [usize; 6] = [0; 6];

        for _ in 0..REPETITIONS {
            let s = collect_shuffle(vec!["0", "1", "2"]);

            match (s[0].as_str(), s[1].as_str(), s[2].as_str()) {
                ("0", "1", "2") => occurrences[0] += 1,
                ("0", "2", "1") => occurrences[1] += 1,
                ("1", "0", "2") => occurrences[2] += 1,
                ("1", "2", "0") => occurrences[3] += 1,
                ("2", "0", "1") => occurrences[4] += 1,
                ("2", "1", "0") => occurrences[5] += 1,
                _ => panic!("Invalid shuffle.")
            }
        }

        for occurrences in occurrences {
            assert!(occurrences >= MIN_EXPECTED);
            assert!(occurrences <= MAX_EXPECTED);
        }
    }
}
